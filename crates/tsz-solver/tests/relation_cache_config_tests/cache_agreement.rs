use super::*;

#[test]
fn assignability_cache_strict_function_types_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let narrow = interner.literal_string("narrow-param");
    let source = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(narrow)],
        TypeId::VOID,
    ));
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(TypeId::STRING)],
        TypeId::VOID,
    ));

    let legacy_bivariant = RelationPolicy::unflagged_compatibility();
    let strict_contravariant =
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_FUNCTION_TYPES);
    let legacy_key =
        RelationCacheKey::for_assignability(source, target, legacy_bivariant.cache_config());
    let strict_key =
        RelationCacheKey::for_assignability(source, target, strict_contravariant.cache_config());

    assert_ne!(
        legacy_key, strict_key,
        "strict function types must occupy a distinct cache slot",
    );

    let legacy_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        legacy_bivariant,
        RelationContext::default(),
    )
    .is_related();
    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_contravariant,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        legacy_uncached,
        "legacy bivariant function-parameter mode should allow the narrow source parameter",
    );
    assert!(
        !strict_uncached,
        "strict function types must reject the narrow source parameter contravariantly",
    );

    let strict_cached = db.is_assignable_to_with_policy(source, target, strict_contravariant);
    assert_eq!(
        strict_cached, strict_uncached,
        "cached strict-function-types result must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_cached),
        "strict-function-types result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(legacy_key),
        None,
        "legacy lookup must not hit the strict-function-types slot",
    );

    let legacy_cached = db.is_assignable_to_with_policy(source, target, legacy_bivariant);
    assert_eq!(
        legacy_cached, legacy_uncached,
        "cached legacy bivariant result must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_assignability_cache(legacy_key),
        Some(legacy_cached),
        "legacy bivariant result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(strict_cached),
        "strict-function-types slot must remain intact after the legacy lookup",
    );
}

// =============================================================================
// 3. Typed policy projection preserves stable external bits
// =============================================================================

#[test]
fn legacy_flag_constants_match_typed_bitflags() {
    // External callers still depend on the `FLAG_*` `u16` constants being
    // numerically stable. Guarantee they line up with the typed layout so
    // that bridges like `pack_relation_flags()` keep working.
    assert_eq!(
        u32::from(RelationCacheKey::FLAG_STRICT_NULL_CHECKS),
        RelationFlags::STRICT_NULL_CHECKS.bits()
    );
    assert_eq!(
        u32::from(RelationCacheKey::FLAG_NO_ERASE_GENERICS),
        RelationFlags::NO_ERASE_GENERICS.bits()
    );
}

#[test]
fn policy_cache_config_preserves_typed_extended_bits() {
    let typed_flags = RelationFlags::STRICT_SUBTYPE_CHECKING
        | RelationFlags::STRICT_ANY_PROPAGATION
        | RelationFlags::SKIP_WEAK_TYPE_CHECKS
        | RelationFlags::ASSUME_RELATED_ON_CYCLE
        | RelationFlags::IN_CALLBACK_PARAM_CHECK
        | RelationFlags::STRICT_READONLY_IDENTITY;

    let config = RelationPolicy::from_relation_flags(typed_flags).cache_config();

    assert!(
        config
            .flags
            .contains(RelationFlags::STRICT_SUBTYPE_CHECKING)
    );
    assert!(config.flags.contains(RelationFlags::STRICT_ANY_PROPAGATION));
    assert!(config.flags.contains(RelationFlags::SKIP_WEAK_TYPE_CHECKS));
    assert!(
        config
            .flags
            .contains(RelationFlags::ASSUME_RELATED_ON_CYCLE)
    );
    assert!(
        config
            .flags
            .contains(RelationFlags::IN_CALLBACK_PARAM_CHECK)
    );
    assert!(
        config
            .flags
            .contains(RelationFlags::STRICT_READONLY_IDENTITY)
    );
}

#[test]
fn policy_cache_config_preserves_all_assigned_typed_bits() {
    let all_flags = RelationFlags::all();
    let config = RelationPolicy::from_relation_flags(all_flags).cache_config();

    assert_eq!(
        config.flags, all_flags,
        "typed flag projection must preserve every assigned relation flag",
    );
}

#[test]
fn relation_policy_from_relation_flags_preserves_typed_bits() {
    let typed_flags = RelationFlags::STRICT_NULL_CHECKS
        | RelationFlags::STRICT_FUNCTION_TYPES
        | RelationFlags::NO_ERASE_GENERICS
        | RelationFlags::IN_CALLBACK_PARAM_CHECK
        | RelationFlags::STRICT_READONLY_IDENTITY;

    let typed = RelationPolicy::from_relation_flags(typed_flags);
    let packed = RelationPolicy::from_flags(typed_flags.bits() as u16);

    assert_eq!(
        typed.cache_config(),
        packed.cache_config(),
        "typed relation flags must produce the same cache config as the legacy edge",
    );
    assert!(
        typed.strict_null_checks(),
        "typed constructor should preserve strict-null policy",
    );
    assert!(
        typed.strict_function_types(),
        "typed constructor should preserve strict-function policy",
    );
    assert!(
        !typed.erase_generics,
        "typed constructor should preserve NO_ERASE_GENERICS",
    );
    assert!(
        typed
            .cache_config()
            .flags
            .contains(RelationFlags::IN_CALLBACK_PARAM_CHECK),
        "typed constructor should preserve transient callback relation mode",
    );
    assert_eq!(
        typed.legacy_packed_flags(),
        typed_flags.bits() as u16,
        "compatibility edges should still be able to observe the packed bit layout",
    );
}

#[test]
fn relation_policy_typed_accessors_preserve_packed_relation_bits() {
    let enabled = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS
            | RelationFlags::STRICT_FUNCTION_TYPES
            | RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES
            | RelationFlags::NO_UNCHECKED_INDEXED_ACCESS
            | RelationFlags::DISABLE_METHOD_BIVARIANCE
            | RelationFlags::ALLOW_VOID_RETURN
            | RelationFlags::ALLOW_BIVARIANT_REST
            | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT
            | RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY
            | RelationFlags::STRICT_READONLY_IDENTITY,
    );
    let disabled = RelationPolicy::from_relation_flags(RelationFlags::empty());

    assert!(enabled.strict_null_checks());
    assert!(enabled.strict_function_types());
    assert!(enabled.exact_optional_property_types());
    assert!(enabled.no_unchecked_indexed_access());
    assert!(enabled.disable_method_bivariance());
    assert!(enabled.allow_void_return());
    assert!(enabled.allow_bivariant_rest());
    assert!(enabled.allow_bivariant_param_count());
    assert!(enabled.allow_erased_generic_signature_retry());
    assert!(enabled.strict_readonly_identity());

    assert!(!disabled.strict_null_checks());
    assert!(!disabled.strict_function_types());
    assert!(!disabled.exact_optional_property_types());
    assert!(!disabled.no_unchecked_indexed_access());
    assert!(!disabled.disable_method_bivariance());
    assert!(!disabled.allow_void_return());
    assert!(!disabled.allow_bivariant_rest());
    assert!(!disabled.allow_bivariant_param_count());
    assert!(!disabled.allow_erased_generic_signature_retry());
    assert!(!disabled.strict_readonly_identity());
}

#[test]
fn relation_policy_legacy_packed_flags_accessor_preserves_input_bits() {
    let packed = RelationCacheKey::FLAG_STRICT_NULL_CHECKS
        | RelationCacheKey::FLAG_ALLOW_VOID_RETURN
        | RelationFlags::STRICT_READONLY_IDENTITY.bits() as u16;
    let policy = RelationPolicy::from_flags(packed);

    assert_eq!(
        policy.legacy_packed_flags(),
        packed,
        "compatibility edges should observe the exact legacy bit layout",
    );
}

#[test]
fn subtype_flags_entrypoint_uses_relation_policy_cache_config() {
    let flags =
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS | RelationCacheKey::FLAG_NO_ERASE_GENERICS;

    let key = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationPolicy::from_flags(flags).cache_config(),
    );

    assert!(
        key.config
            .flags
            .contains(RelationFlags::ASSUME_RELATED_ON_CYCLE),
        "subtype flags entrypoint must preserve RelationPolicy's cycle default",
    );
    assert!(
        key.config.flags.contains(RelationFlags::NO_ERASE_GENERICS),
        "subtype flags entrypoint must preserve explicit packed bits",
    );
}

#[test]
fn assignability_flags_entrypoint_uses_relation_policy_cache_config() {
    let flags = RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES
        | RelationCacheKey::FLAG_EXACT_OPTIONAL_PROPERTY_TYPES;

    let key = RelationCacheKey::for_assignability(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationPolicy::from_flags(flags).cache_config(),
    );

    assert!(
        key.config
            .flags
            .contains(RelationFlags::ASSUME_RELATED_ON_CYCLE),
        "assignability flags entrypoint must preserve RelationPolicy's cycle default",
    );
    assert!(
        !key.config
            .flags
            .contains(RelationFlags::STRICT_ANY_PROPAGATION),
        "assignability flags entrypoint must not infer strict-any from strict function types",
    );
}

#[test]
fn query_cache_relation_misses_insert_policy_shaped_keys() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.literal_string("policy-key-source");
    let flags =
        RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES | RelationCacheKey::FLAG_NO_ERASE_GENERICS;
    let config = RelationPolicy::from_flags(flags).cache_config();

    assert!(db.is_subtype_of_with_policy(
        source,
        TypeId::STRING,
        RelationPolicy::from_flags(flags),
    ));
    assert_eq!(
        db.lookup_subtype_cache(RelationCacheKey::for_subtype(
            source,
            TypeId::STRING,
            config,
        )),
        Some(true),
        "subtype miss path must insert under the policy-derived cache key",
    );

    assert!(db.is_assignable_to_with_policy(
        source,
        TypeId::STRING,
        RelationPolicy::from_flags(flags),
    ));
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            TypeId::STRING,
            config,
        )),
        Some(true),
        "assignability miss path must insert under the policy-derived cache key",
    );
}

#[test]
fn query_cache_typed_policy_entrypoints_insert_policy_shaped_keys() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.literal_string("typed-policy-key-source");
    let policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES)
        .with_strict_any_propagation(true)
        .with_skip_weak_type_checks(true)
        .with_erase_generics(false);
    let config = policy.cache_config();

    assert!(db.is_subtype_of_with_policy(source, TypeId::STRING, policy));
    assert_eq!(
        db.lookup_subtype_cache(RelationCacheKey::for_subtype(
            source,
            TypeId::STRING,
            config,
        )),
        Some(true),
        "typed subtype policy path must insert under the policy-derived cache key",
    );

    assert!(db.is_assignable_to_with_policy(source, TypeId::STRING, policy));
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            TypeId::STRING,
            config,
        )),
        Some(true),
        "typed assignability policy path must insert under the policy-derived cache key",
    );
}

#[test]
fn assignability_cache_strict_any_policy_matches_uncached_relation_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let value = interner.intern_string("value");
    let source = interner.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
    let target = interner.object(vec![PropertyInfo::new(value, TypeId::NUMBER)]);
    let policy = RelationPolicy::default().with_strict_any_propagation(true);

    let uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        policy,
        RelationContext::default(),
    );
    let cached = db.is_assignable_to_with_policy(source, target, policy);
    let cached_again = db.is_assignable_to_with_policy(source, target, policy);
    let stats = db.relation_cache_stats();

    assert_eq!(
        cached,
        uncached.is_related(),
        "cached strict-any assignability must match the uncached relation facade",
    );
    assert_eq!(
        cached_again, cached,
        "second strict-any lookup should reuse the same policy-shaped answer",
    );
    assert!(
        stats.assignability_hits >= 1,
        "second strict-any lookup should hit the assignability cache",
    );
    assert!(
        stats.assignability_misses >= 1,
        "first strict-any lookup should miss before inserting",
    );
    assert!(
        !cached,
        "strict-any policy must not let nested `any` silence the property mismatch",
    );
}

#[test]
fn subtype_cache_top_level_only_any_mode_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let value = interner.intern_string("value");
    let source = interner.object(vec![PropertyInfo::new(value, TypeId::ANY)]);
    let target = interner.object(vec![PropertyInfo::new(value, TypeId::NUMBER)]);

    let all_any_policy =
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::All);
    let top_level_only_policy =
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);
    let all_any_key = RelationCacheKey::for_subtype(source, target, all_any_policy.cache_config());
    let top_level_only_key =
        RelationCacheKey::for_subtype(source, target, top_level_only_policy.cache_config());

    assert_ne!(
        all_any_key, top_level_only_key,
        "top-level-only any propagation must partition subtype cache entries",
    );

    let all_any_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        all_any_policy,
        RelationContext::default(),
    )
    .is_related();
    let top_level_only_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        top_level_only_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        all_any_uncached,
        "ordinary any propagation should let a nested any property satisfy a number property",
    );
    assert!(
        !top_level_only_uncached,
        "top-level-only any propagation must not let nested any silence the property mismatch",
    );

    let all_any_cached = db.is_subtype_of_with_policy(source, target, all_any_policy);
    let top_level_only_cached = db.is_subtype_of_with_policy(source, target, top_level_only_policy);

    assert_eq!(
        all_any_cached, all_any_uncached,
        "cached all-any subtype must match the uncached relation facade",
    );
    assert_eq!(
        top_level_only_cached, top_level_only_uncached,
        "cached top-level-only subtype must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_subtype_cache(all_any_key),
        Some(all_any_cached),
        "all-any policy result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(top_level_only_key),
        Some(top_level_only_cached),
        "top-level-only any policy result must use its own cache slot",
    );
}

#[test]
fn assignability_policy_flip_matches_uncached_relation_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let strict = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let loose = RelationPolicy::unflagged_compatibility();

    let strict_uncached = query_relation(
        &interner,
        TypeId::NULL,
        TypeId::NUMBER,
        RelationKind::Assignable,
        strict,
        RelationContext::default(),
    )
    .is_related();
    let strict_cached = db.is_assignable_to_with_policy(TypeId::NULL, TypeId::NUMBER, strict);

    assert_eq!(
        strict_cached, strict_uncached,
        "cached strict-null assignability must match the uncached typed relation query",
    );

    let loose_uncached = query_relation(
        &interner,
        TypeId::NULL,
        TypeId::NUMBER,
        RelationKind::Assignable,
        loose,
        RelationContext::default(),
    )
    .is_related();
    let loose_cached = db.is_assignable_to_with_policy(TypeId::NULL, TypeId::NUMBER, loose);

    assert_eq!(
        loose_cached, loose_uncached,
        "cached non-strict assignability must match the uncached typed relation query",
    );
    assert_ne!(
        strict_cached, loose_cached,
        "null assignability should differ across strict-null policy slots",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(TypeId::NULL, TypeId::NUMBER, strict),
        strict_uncached,
        "strict slot must remain stable after populating the non-strict slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            TypeId::NULL,
            TypeId::NUMBER,
            strict.cache_config(),
        )),
        Some(strict_uncached),
        "strict policy result must be stored under the strict policy-derived key",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            TypeId::NULL,
            TypeId::NUMBER,
            loose.cache_config(),
        )),
        Some(loose_uncached),
        "non-strict policy result must be stored under the non-strict policy-derived key",
    );
}

#[test]
fn assignability_cache_erase_generics_policy_matches_uncached_relation_query() {
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
        "erased and strict generic-signature policies must occupy distinct cache slots",
    );

    let uncached_erased = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        erased,
        RelationContext::default(),
    );
    let uncached_strict = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict,
        RelationContext::default(),
    );

    assert!(
        uncached_erased.is_related(),
        "erased generic-signature compatibility should allow the relation",
    );
    assert!(
        !uncached_strict.is_related(),
        "strict member compatibility must not promote an outer type parameter into a generic signature",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, strict),
        uncached_strict.is_related(),
        "cached strict generic policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(uncached_strict.is_related()),
        "strict generic result must be stored in the strict slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(erased_key),
        None,
        "erased-generic lookup must not hit the strict slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, erased),
        uncached_erased.is_related(),
        "cached erased generic policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(erased_key),
        Some(uncached_erased.is_related()),
        "erased generic result must be stored in the erased slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(strict_key),
        Some(uncached_strict.is_related()),
        "strict generic slot must remain intact after the erased lookup",
    );
}

#[test]
fn assignability_cache_erased_generic_retry_matches_uncached_relation_query() {
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
        RelationPolicy::from_flags(RelationCacheKey::FLAG_ALLOW_ERASED_GENERIC_SIGNATURE_RETRY);
    let no_retry_key = RelationCacheKey::for_assignability(source, target, no_retry.cache_config());
    let retry_key = RelationCacheKey::for_assignability(source, target, retry.cache_config());

    assert_ne!(
        no_retry_key, retry_key,
        "erased generic retry policy must occupy a distinct cache slot",
    );

    let uncached_no_retry = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        no_retry,
        RelationContext::default(),
    );
    let uncached_retry = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        retry,
        RelationContext::default(),
    );

    assert!(
        !uncached_no_retry.is_related(),
        "contextual inference should reject the unequal-arity generic signatures before retry",
    );
    assert!(
        uncached_retry.is_related(),
        "erased generic retry should allow the unequal-arity signatures",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, no_retry),
        uncached_no_retry.is_related(),
        "cached no-retry policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(no_retry_key),
        Some(uncached_no_retry.is_related()),
        "no-retry result must be stored in the no-retry slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(retry_key),
        None,
        "retry lookup must not hit the no-retry slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, retry),
        uncached_retry.is_related(),
        "cached retry policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(retry_key),
        Some(uncached_retry.is_related()),
        "retry result must be stored in the retry slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(no_retry_key),
        Some(uncached_no_retry.is_related()),
        "no-retry slot must remain intact after the retry lookup",
    );
}

#[test]
fn assignability_cache_allow_bivariant_rest_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.function(FunctionShape::new(
        vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        TypeId::VOID,
    ));
    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo {
            name: None,
            type_id: rest_any,
            optional: false,
            rest: true,
        }],
        TypeId::VOID,
    ));
    let strict_without_rest = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::STRICT_NULL_CHECKS,
    )
    .with_strict_any_propagation(true)
    .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);
    let strict_with_rest = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES
            | RelationFlags::STRICT_NULL_CHECKS
            | RelationFlags::ALLOW_BIVARIANT_REST,
    )
    .with_strict_any_propagation(true)
    .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);

    let uncached_without_rest = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_without_rest,
        RelationContext::default(),
    )
    .is_related();
    let cached_without_rest = db.is_assignable_to_with_policy(source, target, strict_without_rest);

    assert_eq!(
        cached_without_rest, uncached_without_rest,
        "cached assignability without bivariant rest must match the uncached relation facade",
    );

    let uncached_with_rest = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_with_rest,
        RelationContext::default(),
    )
    .is_related();
    let cached_with_rest = db.is_assignable_to_with_policy(source, target, strict_with_rest);

    assert_eq!(
        cached_with_rest, uncached_with_rest,
        "cached assignability with bivariant rest must match the uncached relation facade",
    );
    assert!(
        !cached_without_rest,
        "without ALLOW_BIVARIANT_REST, strict-any assignability must compare extra parameters normally",
    );
    assert!(
        cached_with_rest,
        "with ALLOW_BIVARIANT_REST, strict-any assignability should accept the rest-any target",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            target,
            strict_without_rest.cache_config(),
        )),
        Some(cached_without_rest),
        "non-bivariant-rest policy result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            target,
            strict_with_rest.cache_config(),
        )),
        Some(cached_with_rest),
        "bivariant-rest policy result must use its own cache slot",
    );
}

#[test]
fn assignability_cache_top_rest_any_rejects_never_source_param() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let rest_any = interner.array(TypeId::ANY);
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo {
            name: None,
            type_id: rest_any,
            optional: false,
            rest: true,
        }],
        TypeId::VOID,
    ));
    let zero_arg_source = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let never_source = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(TypeId::NEVER)],
        TypeId::VOID,
    ));
    let compiler_like_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::STRICT_NULL_CHECKS,
    );

    let zero_arg_uncached = query_relation(
        &interner,
        zero_arg_source,
        target,
        RelationKind::Assignable,
        compiler_like_policy,
        RelationContext::default(),
    )
    .is_related();
    let zero_arg_cached =
        db.is_assignable_to_with_policy(zero_arg_source, target, compiler_like_policy);
    assert_eq!(
        zero_arg_cached, zero_arg_uncached,
        "cached zero-arg function assignability must match the uncached relation facade",
    );
    assert!(
        zero_arg_cached,
        "zero-arg functions remain assignable to top rest-`any` targets",
    );

    let never_uncached = query_relation(
        &interner,
        never_source,
        target,
        RelationKind::Assignable,
        compiler_like_policy,
        RelationContext::default(),
    )
    .is_related();
    let never_cached = db.is_assignable_to_with_policy(never_source, target, compiler_like_policy);
    assert_eq!(
        never_cached, never_uncached,
        "cached `never`-parameter assignability must match the uncached relation facade",
    );
    assert!(
        !never_cached,
        "`(a: never) => void` is not assignable to `(...args: any[]) => void`",
    );
}

#[test]
fn assignability_cache_allow_bivariant_param_count_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.function(FunctionShape::new(
        vec![
            ParamInfo::unnamed(TypeId::STRING),
            ParamInfo::unnamed(TypeId::NUMBER),
        ],
        TypeId::VOID,
    ));
    let target = interner.function(FunctionShape::new(
        vec![ParamInfo::unnamed(TypeId::STRING)],
        TypeId::VOID,
    ));
    let strict_count_policy = RelationPolicy::unflagged_compatibility();
    let bivariant_count_policy =
        RelationPolicy::from_relation_flags(RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT);

    let strict_count_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        strict_count_policy,
        RelationContext::default(),
    )
    .is_related();
    let strict_count_cached = db.is_assignable_to_with_policy(source, target, strict_count_policy);

    assert_eq!(
        strict_count_cached, strict_count_uncached,
        "cached strict parameter-count assignability must match the uncached relation facade",
    );

    let bivariant_count_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        bivariant_count_policy,
        RelationContext::default(),
    )
    .is_related();
    let bivariant_count_cached =
        db.is_assignable_to_with_policy(source, target, bivariant_count_policy);

    assert_eq!(
        bivariant_count_cached, bivariant_count_uncached,
        "cached bivariant parameter-count assignability must match the uncached relation facade",
    );
    assert!(
        !strict_count_cached,
        "without ALLOW_BIVARIANT_PARAM_COUNT, extra required source parameters must be rejected",
    );
    assert!(
        bivariant_count_cached,
        "ALLOW_BIVARIANT_PARAM_COUNT should allow the bivariant comparison to ignore extra required source parameters",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            target,
            strict_count_policy.cache_config(),
        )),
        Some(strict_count_cached),
        "strict parameter-count policy result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            target,
            bivariant_count_policy.cache_config(),
        )),
        Some(bivariant_count_cached),
        "bivariant parameter-count policy result must use its own cache slot",
    );
}

#[test]
fn assignability_cache_bivariant_param_count_respects_disabled_method_bivariance() {
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

    let bivariant_method_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT,
    );
    let disabled_method_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES
            | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT
            | RelationFlags::DISABLE_METHOD_BIVARIANCE,
    );

    let bivariant_method_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        bivariant_method_policy,
        RelationContext::default(),
    )
    .is_related();
    let bivariant_method_cached =
        db.is_assignable_to_with_policy(source, target, bivariant_method_policy);

    assert_eq!(
        bivariant_method_cached, bivariant_method_uncached,
        "cached method-bivariant parameter-count assignability must match the uncached relation facade",
    );
    assert!(
        bivariant_method_cached,
        "ALLOW_BIVARIANT_PARAM_COUNT should ignore extra required method parameters when method bivariance is enabled",
    );

    let disabled_method_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        disabled_method_policy,
        RelationContext::default(),
    )
    .is_related();
    let disabled_method_cached =
        db.is_assignable_to_with_policy(source, target, disabled_method_policy);

    assert_eq!(
        disabled_method_cached, disabled_method_uncached,
        "cached disabled-method-bivariance parameter-count assignability must match the uncached relation facade",
    );
    assert!(
        !disabled_method_cached,
        "DISABLE_METHOD_BIVARIANCE must also disable the method branch of ALLOW_BIVARIANT_PARAM_COUNT",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            target,
            bivariant_method_policy.cache_config(),
        )),
        Some(bivariant_method_cached),
        "method-bivariant parameter-count result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(RelationCacheKey::for_assignability(
            source,
            target,
            disabled_method_policy.cache_config(),
        )),
        Some(disabled_method_cached),
        "disabled-method-bivariance result must use its own cache slot",
    );
}

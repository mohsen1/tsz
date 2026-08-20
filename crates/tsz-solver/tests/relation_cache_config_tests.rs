//! Regression tests for `RelationCacheConfig` / `RelationCacheKey` behavior.
//!
//! These tests pin down the cache-partitioning contract:
//!
//! 1. Every behavior-affecting configuration change must produce a distinct
//!    [`RelationCacheKey`] so that results cannot accidentally share a slot.
//! 2. `skip_weak_type_checks` and `erase_generics` must partition cache
//!    entries (they actually change the relation outcome).
//! 3. Different `any_propagation_mode` values must produce distinct keys.
//! 4. Every `RelationFlag` bit produces a distinct key, including
//!    `ALLOW_ERASED_GENERIC_SIGNATURE_RETRY`, `IN_CALLBACK_PARAM_CHECK`,
//!    `STRICT_READONLY_IDENTITY`, and `PROVISIONAL_REST_UNION`.
//! 5. Typed `RelationPolicy` query-cache entrypoints insert under
//!    policy-derived cache keys.
//! 6. The typed no-flags compatibility constructor remains equivalent to the
//!    legacy `RelationPolicy::from_flags(0)` constructor, without collapsing
//!    into `RelationPolicy::default()`.

use super::*;
use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::computation::TypeEnvironment;
use crate::def::{DefId, DefKind};
use crate::intern::TypeInterner;
use crate::relations::relation_queries::{
    RelationContext, RelationKind, RelationPolicy, query_relation, query_relation_with_resolver,
};
use crate::relations::subtype::AnyPropagationMode;
use crate::types::{
    CachedAnyMode, FunctionShape, ParamInfo, PropertyInfo, RelationCacheConfig, RelationCacheKey,
    RelationCacheKind, RelationFlags, TupleElement, TypeData, TypeParamInfo, TypeParamOrigin,
};

#[path = "relation_cache_config_tests/cache_agreement.rs"]
mod cache_agreement;

/// Assert that two `RelationPolicy` configurations produce distinct
/// assignability cache keys for the same `(STRING, NUMBER)` pair. Centralises
/// the build-two-keys / `assert_ne!` shape used by the per-flag partition
/// regression tests below.
///
/// `pub(crate)`: also reused by the exhaustive-destructuring tripwire in
/// `relations::relation_queries::tests`, which must live next to
/// `RelationPolicy` to see its private `flags` field.
pub(crate) fn assert_assignability_partitions(name: &str, on: RelationPolicy, off: RelationPolicy) {
    let key_on =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, on.cache_config());
    let key_off =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, off.cache_config());
    assert_ne!(key_on, key_off, "{name} must partition the cache");
}

/// Subtype-cache counterpart of [`assert_assignability_partitions`].
pub(crate) fn assert_subtype_partitions(name: &str, on: RelationPolicy, off: RelationPolicy) {
    let key_on = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, on.cache_config());
    let key_off = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, off.cache_config());
    assert_ne!(key_on, key_off, "{name} must partition the cache");
}

/// Asserts that a flag reachable only via the packed `u16` path partitions the
/// subtype cache: enabling the flag must produce a different key than disabling it.
fn assert_packed_flag_partitions(name: &str, flag_bits: u16) {
    assert_subtype_partitions(
        name,
        RelationPolicy::from_flags(flag_bits),
        RelationPolicy::from_flags(0),
    );
}

#[test]
fn unflagged_compatibility_policy_matches_empty_legacy_flags() {
    let typed = RelationPolicy::unflagged_compatibility();
    let legacy = RelationPolicy::from_flags(0);

    assert_eq!(
        typed, legacy,
        "typed no-flags compatibility policy must preserve the legacy packed no-flags behavior",
    );
    assert_eq!(
        typed.cache_config(),
        legacy.cache_config(),
        "typed no-flags compatibility policy must use the legacy no-flags cache slot",
    );
    assert_ne!(
        typed.cache_config(),
        RelationPolicy::default().cache_config(),
        "historical no-flags compatibility remains distinct from the strict-null default policy",
    );
}

#[test]
fn relation_cache_config_does_not_expose_raw_flags_constructor() {
    let source = include_str!("../src/types/relation_cache.rs");

    assert!(
        !source.contains("pub const fn from_flags(flags: RelationFlags) -> Self"),
        "RelationCacheConfig must not expose a raw flags constructor; use RelationPolicy::from_relation_flags(...).cache_config() so typed policy defaults and overrides stay canonical",
    );
}

#[test]
fn relation_cache_config_does_not_expose_raw_constructor() {
    let source = include_str!("../src/types/relation_cache.rs");
    let impl_start = source
        .find("impl RelationCacheConfig")
        .expect("types/relation_cache.rs must keep RelationCacheConfig impl");
    let key_start = source[impl_start..]
        .find("pub struct RelationCacheKey")
        .map(|offset| impl_start + offset)
        .expect("RelationCacheKey should follow RelationCacheConfig");
    let config_impl = &source[impl_start..key_start];

    assert!(
        config_impl.contains("pub(crate) const fn new"),
        "raw RelationCacheConfig construction should stay internal to tsz-solver",
    );
    assert!(
        !config_impl.contains("pub const fn new"),
        "external callers should project cache configs through RelationPolicy",
    );
}

#[test]
fn query_trace_relation_cache_config_fixture_uses_policy_projection() {
    let source = include_str!("../src/caches/query_trace.rs");

    assert!(
        source.contains("RelationPolicy::from_relation_flags"),
        "query trace relation config fixtures should use the typed relation policy projection",
    );
    assert!(
        !source.contains("RelationCacheConfig::new"),
        "query trace should not hand-build relation cache configs; use RelationPolicy::cache_config()",
    );
}

#[test]
fn legacy_flag_constructor_stores_typed_relation_flags() {
    let policy = RelationPolicy::from_flags(
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS
            | RelationCacheKey::FLAG_DISABLE_METHOD_BIVARIANCE,
    );
    let config = policy.cache_config();

    assert!(config.flags.contains(RelationFlags::STRICT_NULL_CHECKS));
    assert!(
        config
            .flags
            .contains(RelationFlags::DISABLE_METHOD_BIVARIANCE)
    );
}

// =============================================================================
// 1. Every behavior-affecting setting must change the key
// =============================================================================

#[test]
fn each_relation_flag_bit_produces_a_distinct_key() {
    let base = RelationCacheConfig::default();
    let base_key = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, base);

    // Every single-bit flip must produce a fresh cache key.
    let single_bits = [
        RelationFlags::STRICT_NULL_CHECKS,
        RelationFlags::STRICT_FUNCTION_TYPES,
        RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES,
        RelationFlags::NO_UNCHECKED_INDEXED_ACCESS,
        RelationFlags::DISABLE_METHOD_BIVARIANCE,
        RelationFlags::ALLOW_VOID_RETURN,
        RelationFlags::ALLOW_BIVARIANT_REST,
        RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT,
        RelationFlags::NO_ERASE_GENERICS,
        RelationFlags::SKIP_WEAK_TYPE_CHECKS,
        RelationFlags::ASSUME_RELATED_ON_CYCLE,
        RelationFlags::ASSUME_RELATED_ON_DEPTH,
        // Transient flags set during checker execution — they reach the cache
        // via packed `u16` flags rather than a typed builder field, but they
        // must still partition the cache to keep distinct relation passes in
        // separate slots.
        RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY,
        RelationFlags::IN_CALLBACK_PARAM_CHECK,
        RelationFlags::STRICT_READONLY_IDENTITY,
        RelationFlags::PROVISIONAL_REST_UNION,
    ];

    for bit in single_bits {
        let flipped = RelationPolicy::from_relation_flags(base.flags | bit)
            .cache_config()
            .with_any_mode(base.any_mode);
        let flipped_key = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, flipped);
        assert_ne!(
            base_key, flipped_key,
            "flipping `{bit:?}` must change the cache key",
        );
    }
}

#[test]
fn different_relation_kinds_produce_distinct_keys() {
    let config = RelationCacheConfig::default();
    let sub = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, config);
    let assign = RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, config);
    let identical = RelationCacheKey::for_identical(TypeId::STRING, TypeId::NUMBER, config);

    assert_ne!(sub, assign);
    assert_ne!(sub, identical);
    assert_ne!(assign, identical);
    assert_eq!(sub.relation, RelationCacheKind::Subtype);
    assert_eq!(assign.relation, RelationCacheKind::Assignable);
    assert_eq!(identical.relation, RelationCacheKind::Identical);
}

#[test]
fn query_cache_relation_kinds_match_uncached_relation_queries() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let optional = interner.intern_string("relationKindOptional");
    let unrelated = interner.intern_string("relationKindUnrelated");
    let source = interner.object(vec![PropertyInfo::new(unrelated, TypeId::BOOLEAN)]);
    let target = interner.object(vec![PropertyInfo::opt(optional, TypeId::NUMBER)]);
    let policy = RelationPolicy::default();
    let subtype_key = RelationCacheKey::for_subtype(source, target, policy.cache_config());
    let assignability_key =
        RelationCacheKey::for_assignability(source, target, policy.cache_config());

    assert_ne!(
        subtype_key, assignability_key,
        "subtype and assignability must occupy distinct cache slots",
    );

    let uncached_subtype = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        policy,
        RelationContext::default(),
    )
    .is_related();
    let uncached_assignability = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        uncached_subtype,
        "structural subtype should accept an object against an all-optional target",
    );
    assert!(
        !uncached_assignability,
        "assignability should reject the unrelated source as a weak-type violation",
    );

    let subtype_cached = db.is_subtype_of_with_policy(source, target, policy);
    assert_eq!(
        subtype_cached, uncached_subtype,
        "cached subtype result must match the uncached subtype relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        Some(subtype_cached),
        "subtype result must be stored in the subtype cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        None,
        "assignability lookup must not hit the populated subtype slot",
    );

    let assignability_cached = db.is_assignable_to_with_policy(source, target, policy);
    assert_eq!(
        assignability_cached, uncached_assignability,
        "cached assignability result must match the uncached assignability relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(assignability_key),
        Some(assignability_cached),
        "assignability result must be stored in the assignability cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(subtype_key),
        Some(subtype_cached),
        "subtype slot must remain intact after the assignability lookup",
    );
}

#[test]
fn provisional_rest_union_policy_partitions_query_cache_in_both_orders() {
    let interner = TypeInterner::new();
    let file = interner.intern_string("provisional-rest-cache.ts");
    let rest_param = interner.fresh_type_param(TypeParamInfo {
        name: interner.intern_string("Values"),
        constraint: Some(interner.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
        origin: TypeParamOrigin::DeclScoped { file, node: 1 },
    });
    let rest_function = |rest_type| {
        interner.function(FunctionShape {
            type_params: vec![],
            params: vec![ParamInfo {
                name: Some(interner.intern_string("values")),
                type_id: rest_type,
                optional: false,
                rest: true,
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    };
    let source = rest_function(rest_param);
    let fixed = interner.tuple(vec![TupleElement::fixed(rest_param)]);
    let spread = interner.tuple(vec![TupleElement {
        type_id: rest_param,
        name: None,
        optional: false,
        rest: true,
    }]);
    let target = rest_function(interner.union_preserve_members(vec![fixed, spread]));
    let ordinary = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_FUNCTION_TYPES.union(RelationFlags::ALLOW_BIVARIANT_REST),
    );
    let provisional = ordinary.with_provisional_rest_union(true);

    let db = QueryCache::new(&interner);
    assert!(!db.is_assignable_to_with_policy(source, target, ordinary));
    assert!(db.is_assignable_to_with_policy(source, target, provisional));
    assert!(!db.is_assignable_to_with_policy(source, target, ordinary));

    let ordinary_key = RelationCacheKey::for_assignability(source, target, ordinary.cache_config());
    let provisional_key =
        RelationCacheKey::for_assignability(source, target, provisional.cache_config());
    assert_eq!(db.lookup_assignability_cache(ordinary_key), Some(false));
    assert_eq!(db.lookup_assignability_cache(provisional_key), Some(true));

    let reverse_db = QueryCache::new(&interner);
    assert!(reverse_db.is_assignable_to_with_policy(source, target, provisional));
    assert!(!reverse_db.is_assignable_to_with_policy(source, target, ordinary));
    assert!(reverse_db.is_assignable_to_with_policy(source, target, provisional));
}

#[test]
fn any_propagation_mode_differences_produce_distinct_keys() {
    let any_modes = [
        CachedAnyMode::All,
        CachedAnyMode::TopLevelOnlyAtTop,
        CachedAnyMode::TopLevelOnlyNested,
        CachedAnyMode::AnySourceNotRelated,
    ];
    for (i, &a) in any_modes.iter().enumerate() {
        for (j, &b) in any_modes.iter().enumerate() {
            let ka = RelationCacheKey::for_subtype(
                TypeId::STRING,
                TypeId::NUMBER,
                RelationPolicy::unflagged_compatibility()
                    .cache_config()
                    .with_any_mode(a),
            );
            let kb = RelationCacheKey::for_subtype(
                TypeId::STRING,
                TypeId::NUMBER,
                RelationPolicy::unflagged_compatibility()
                    .cache_config()
                    .with_any_mode(b),
            );
            if i == j {
                assert_eq!(ka, kb, "same any_mode should produce the same key");
            } else {
                assert_ne!(
                    ka, kb,
                    "different any_mode values ({a:?} vs {b:?}) must produce distinct keys"
                );
            }
        }
    }
}

#[test]
fn skip_weak_type_checks_partitions_cache_entries() {
    assert_assignability_partitions(
        "skip_weak_type_checks",
        RelationPolicy::default().with_skip_weak_type_checks(false),
        RelationPolicy::default().with_skip_weak_type_checks(true),
    );
}

#[test]
fn assignability_cache_skip_weak_type_policy_matches_uncached_relation_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let optional = interner.intern_string("optional");
    let unrelated = interner.intern_string("unrelated");
    let source = interner.object(vec![PropertyInfo::new(unrelated, TypeId::BOOLEAN)]);
    let target = interner.object(vec![PropertyInfo::opt(optional, TypeId::NUMBER)]);

    let enforced = RelationPolicy::default().with_skip_weak_type_checks(false);
    let skipped = RelationPolicy::default().with_skip_weak_type_checks(true);
    let enforced_key = RelationCacheKey::for_assignability(source, target, enforced.cache_config());
    let skipped_key = RelationCacheKey::for_assignability(source, target, skipped.cache_config());

    assert_ne!(
        enforced_key, skipped_key,
        "weak-type enforcement and skipped-weak policies must occupy distinct cache slots",
    );

    let uncached_enforced = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        enforced,
        RelationContext::default(),
    )
    .is_related();
    let uncached_skipped = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        skipped,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !uncached_enforced,
        "weak-type enforcement should reject an unrelated object source",
    );
    assert!(
        uncached_skipped,
        "skipping weak-type checks should leave the ordinary optional-property relation assignable",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, enforced),
        uncached_enforced,
        "cached weak-type-enforced policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(enforced_key),
        Some(uncached_enforced),
        "weak-type-enforced result must be stored in the enforced slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(skipped_key),
        None,
        "skipped-weak lookup must not hit the enforced slot",
    );

    assert_eq!(
        db.is_assignable_to_with_policy(source, target, skipped),
        uncached_skipped,
        "cached skipped-weak policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_assignability_cache(skipped_key),
        Some(uncached_skipped),
        "skipped-weak result must be stored in the skipped slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(enforced_key),
        Some(uncached_enforced),
        "weak-type-enforced slot must remain intact after the skipped lookup",
    );
}

#[test]
fn erase_generics_partitions_cache_entries() {
    assert_subtype_partitions(
        "erase_generics",
        RelationPolicy::default().with_erase_generics(true),
        RelationPolicy::default().with_erase_generics(false),
    );
}

#[test]
fn assignability_cache_no_unchecked_indexed_access_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let array = interner.array(TypeId::STRING);
    let indexed_read = interner.intern(TypeData::IndexAccess(array, TypeId::NUMBER));

    let checked_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let unchecked_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::NO_UNCHECKED_INDEXED_ACCESS,
    );
    let checked_key = RelationCacheKey::for_assignability(
        indexed_read,
        TypeId::STRING,
        checked_policy.cache_config(),
    );
    let unchecked_key = RelationCacheKey::for_assignability(
        indexed_read,
        TypeId::STRING,
        unchecked_policy.cache_config(),
    );

    assert_ne!(
        checked_key, unchecked_key,
        "indexed-access read policy must partition assignability cache entries",
    );

    let checked_uncached = query_relation(
        &interner,
        indexed_read,
        TypeId::STRING,
        RelationKind::Assignable,
        checked_policy,
        RelationContext::default(),
    )
    .is_related();
    let unchecked_uncached = query_relation(
        &interner,
        indexed_read,
        TypeId::STRING,
        RelationKind::Assignable,
        unchecked_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        checked_uncached,
        "without noUncheckedIndexedAccess, array[number] should read as string",
    );
    assert!(
        !unchecked_uncached,
        "with noUncheckedIndexedAccess under strict null checks, array[number] should include undefined",
    );

    let checked_cached =
        db.is_assignable_to_with_policy(indexed_read, TypeId::STRING, checked_policy);
    let unchecked_cached =
        db.is_assignable_to_with_policy(indexed_read, TypeId::STRING, unchecked_policy);

    assert_eq!(
        checked_cached, checked_uncached,
        "cached checked indexed-access assignability must match the uncached relation facade",
    );
    assert_eq!(
        unchecked_cached, unchecked_uncached,
        "cached unchecked indexed-access assignability must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_assignability_cache(checked_key),
        Some(checked_cached),
        "checked indexed-access policy result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(unchecked_key),
        Some(unchecked_cached),
        "unchecked indexed-access policy result must use its own cache slot",
    );
}

#[test]
fn assignability_cache_exact_optional_property_types_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let property = interner.intern_string("value");
    let source = interner.object(vec![PropertyInfo::new(property, TypeId::UNDEFINED)]);
    let target = interner.object(vec![PropertyInfo::opt(property, TypeId::NUMBER)]);

    let loose_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let exact_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES,
    );
    let loose_key =
        RelationCacheKey::for_assignability(source, target, loose_policy.cache_config());
    let exact_key =
        RelationCacheKey::for_assignability(source, target, exact_policy.cache_config());

    assert_ne!(
        loose_key, exact_key,
        "exact optional property policy must partition assignability cache entries",
    );

    let loose_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        loose_policy,
        RelationContext::default(),
    )
    .is_related();
    let exact_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Assignable,
        exact_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        loose_uncached,
        "without exactOptionalPropertyTypes, a present undefined value should satisfy an optional property",
    );
    assert!(
        !exact_uncached,
        "with exactOptionalPropertyTypes, a present undefined value must not satisfy an optional number property",
    );

    let loose_cached = db.is_assignable_to_with_policy(source, target, loose_policy);
    let exact_cached = db.is_assignable_to_with_policy(source, target, exact_policy);

    assert_eq!(
        loose_cached, loose_uncached,
        "cached loose optional-property assignability must match the uncached relation facade",
    );
    assert_eq!(
        exact_cached, exact_uncached,
        "cached exact optional-property assignability must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_assignability_cache(loose_key),
        Some(loose_cached),
        "loose optional-property policy result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_assignability_cache(exact_key),
        Some(exact_cached),
        "exact optional-property policy result must use its own cache slot",
    );
}

#[test]
fn subtype_cache_allow_void_return_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let source = interner.function(FunctionShape::new(vec![], TypeId::STRING));
    let target = interner.function(FunctionShape::new(vec![], TypeId::VOID));

    let strict_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let void_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::ALLOW_VOID_RETURN,
    );
    let strict_key = RelationCacheKey::for_subtype(source, target, strict_policy.cache_config());
    let void_key = RelationCacheKey::for_subtype(source, target, void_policy.cache_config());

    assert_ne!(
        strict_key, void_key,
        "void-return exception policy must partition subtype cache entries",
    );

    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        strict_policy,
        RelationContext::default(),
    )
    .is_related();
    let void_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        void_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        !strict_uncached,
        "without ALLOW_VOID_RETURN, a string-returning source must not satisfy a void-returning target",
    );
    assert!(
        void_uncached,
        "with ALLOW_VOID_RETURN, a non-void source return should satisfy a void target return",
    );

    let strict_cached = db.is_subtype_of_with_policy(source, target, strict_policy);
    let void_cached = db.is_subtype_of_with_policy(source, target, void_policy);

    assert_eq!(
        strict_cached, strict_uncached,
        "cached strict void-return subtype must match the uncached relation facade",
    );
    assert_eq!(
        void_cached, void_uncached,
        "cached void-exception subtype must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_subtype_cache(strict_key),
        Some(strict_cached),
        "strict void-return policy result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(void_key),
        Some(void_cached),
        "void-exception policy result must use its own cache slot",
    );
}

#[test]
fn subtype_cache_strict_readonly_identity_matches_uncached_policy() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let property = interner.intern_string("value");
    let source = interner.object(vec![PropertyInfo::readonly(property, TypeId::STRING)]);
    let target = interner.object(vec![PropertyInfo::new(property, TypeId::STRING)]);

    let ordinary_policy = RelationPolicy::from_relation_flags(RelationFlags::STRICT_NULL_CHECKS);
    let readonly_identity_policy = RelationPolicy::from_relation_flags(
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::STRICT_READONLY_IDENTITY,
    );
    let ordinary_key =
        RelationCacheKey::for_subtype(source, target, ordinary_policy.cache_config());
    let readonly_identity_key =
        RelationCacheKey::for_subtype(source, target, readonly_identity_policy.cache_config());

    assert_ne!(
        ordinary_key, readonly_identity_key,
        "strict readonly identity policy must partition subtype cache entries",
    );

    let ordinary_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        ordinary_policy,
        RelationContext::default(),
    )
    .is_related();
    let readonly_identity_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        readonly_identity_policy,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        ordinary_uncached,
        "ordinary relation mode should allow readonly source properties to satisfy mutable targets",
    );
    assert!(
        !readonly_identity_uncached,
        "strict readonly identity mode must treat readonly mismatch as relation-significant",
    );

    let ordinary_cached = db.is_subtype_of_with_policy(source, target, ordinary_policy);
    let readonly_identity_cached =
        db.is_subtype_of_with_policy(source, target, readonly_identity_policy);

    assert_eq!(
        ordinary_cached, ordinary_uncached,
        "cached ordinary readonly subtype must match the uncached relation facade",
    );
    assert_eq!(
        readonly_identity_cached, readonly_identity_uncached,
        "cached strict-readonly subtype must match the uncached relation facade",
    );
    assert_eq!(
        db.lookup_subtype_cache(ordinary_key),
        Some(ordinary_cached),
        "ordinary readonly policy result must use its own cache slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(readonly_identity_key),
        Some(readonly_identity_cached),
        "strict readonly identity policy result must use its own cache slot",
    );
}

#[test]
fn assume_related_on_cycle_partitions_cache_entries() {
    assert_subtype_partitions(
        "assume_related_on_cycle",
        RelationPolicy::default().with_assume_related_on_cycle(true),
        RelationPolicy::default().with_assume_related_on_cycle(false),
    );
}

#[test]
fn assume_related_on_depth_partitions_cache_entries() {
    assert_subtype_partitions(
        "assume_related_on_depth",
        RelationPolicy::default().with_assume_related_on_depth(true),
        RelationPolicy::default().with_assume_related_on_depth(false),
    );
}

#[test]
fn subtype_cache_assume_related_on_cycle_policy_matches_uncached_relation_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let mut env = TypeEnvironment::new();

    let left_def = DefId(9101);
    let right_def = DefId(9102);
    let next = interner.intern_string("next");

    let left = interner.lazy(left_def);
    let right = interner.lazy(right_def);
    env.insert_def(
        left_def,
        interner.object(vec![PropertyInfo::new(next, left)]),
    );
    env.insert_def(
        right_def,
        interner.object(vec![PropertyInfo::new(next, right)]),
    );
    env.insert_def_kind(left_def, DefKind::TypeAlias);
    env.insert_def_kind(right_def, DefKind::TypeAlias);

    let assume = RelationPolicy::default()
        .with_assume_related_on_cycle(true)
        .with_assume_related_on_depth(false);
    let reject = RelationPolicy::default()
        .with_assume_related_on_cycle(false)
        .with_assume_related_on_depth(false);
    let context = RelationContext {
        query_db: Some(&db),
        ..RelationContext::default()
    };

    let assume_uncached = query_relation_with_resolver(
        &interner,
        &env,
        left,
        right,
        RelationKind::Subtype,
        assume,
        RelationContext::default(),
    )
    .is_related();
    let reject_uncached = query_relation_with_resolver(
        &interner,
        &env,
        left,
        right,
        RelationKind::Subtype,
        reject,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        assume_uncached,
        "recursive aliases should remain coinductively related when only depth overflow is strict",
    );
    assert!(
        !reject_uncached,
        "disabling the cycle assumption should reject the same recursive alias pair",
    );

    let reject_key = RelationCacheKey::for_subtype(left, right, reject.cache_config());
    let assume_key = RelationCacheKey::for_subtype(left, right, assume.cache_config());
    assert_ne!(
        reject_key, assume_key,
        "cycle-assuming and cycle-rejecting policies must occupy distinct cache slots",
    );

    let reject_cached = query_relation_with_resolver(
        &interner,
        &env,
        left,
        right,
        RelationKind::Subtype,
        reject,
        context,
    )
    .is_related();

    assert_eq!(
        reject_cached, reject_uncached,
        "cached cycle-rejecting policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(reject_key),
        Some(reject_uncached),
        "cycle-rejecting result must be stored in the rejecting slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(assume_key),
        None,
        "cycle-assuming lookup must not hit the rejecting slot",
    );

    let assume_cached = query_relation_with_resolver(
        &interner,
        &env,
        left,
        right,
        RelationKind::Subtype,
        assume,
        context,
    )
    .is_related();

    assert_eq!(
        assume_cached, assume_uncached,
        "cached cycle-assuming policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(assume_key),
        Some(assume_uncached),
        "cycle-assuming result must be stored in the assuming slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(reject_key),
        Some(reject_uncached),
        "cycle-rejecting slot must remain intact after the assuming lookup",
    );
}

#[test]
fn any_propagation_mode_partitions_cache_entries_via_policy() {
    assert_subtype_partitions(
        "any_propagation_mode",
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::All),
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::TopLevelOnly),
    );
}

// Flags that reach the cache key through the packed `u16` path rather than a
// typed `RelationPolicy` builder field. Verify they partition entries just like
// the typed-builder flags above.

#[test]
fn allow_erased_generic_signature_retry_partitions_cache_entries() {
    // Set transiently inside `SubtypeChecker` to permit a second pass with
    // erased generic signatures; retry-mode results must live in a separate slot.
    assert_packed_flag_partitions(
        "allow_erased_generic_signature_retry",
        RelationCacheKey::FLAG_ALLOW_ERASED_GENERIC_SIGNATURE_RETRY,
    );
}

#[test]
fn in_callback_param_check_partitions_cache_entries() {
    // Set transiently during function-signature comparison; callback-mode
    // results must live in a separate slot from ordinary comparisons.
    assert_subtype_partitions(
        "in_callback_param_check",
        RelationPolicy::from_relation_flags(RelationFlags::IN_CALLBACK_PARAM_CHECK),
        RelationPolicy::unflagged_compatibility(),
    );
}

#[test]
fn strict_readonly_identity_partitions_cache_entries() {
    // Toggled during conditional-type distribution; results computed under
    // this mode must not share a slot with ordinary relation results.
    assert_subtype_partitions(
        "strict_readonly_identity",
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_READONLY_IDENTITY),
        RelationPolicy::unflagged_compatibility(),
    );
}

#[test]
fn subtype_cache_strict_readonly_identity_policy_matches_uncached_relation_query() {
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let prop = interner.intern_string("value");
    let source = interner.object(vec![PropertyInfo::readonly(prop, TypeId::NUMBER)]);
    let target = interner.object(vec![PropertyInfo::new(prop, TypeId::NUMBER)]);

    let ordinary = RelationPolicy::unflagged_compatibility();
    let strict_readonly =
        RelationPolicy::from_relation_flags(RelationFlags::STRICT_READONLY_IDENTITY);
    let ordinary_key = RelationCacheKey::for_subtype(source, target, ordinary.cache_config());
    let strict_key = RelationCacheKey::for_subtype(source, target, strict_readonly.cache_config());

    assert_ne!(
        ordinary_key, strict_key,
        "ordinary and strict-readonly identity policies must occupy distinct cache slots",
    );

    let ordinary_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        ordinary,
        RelationContext::default(),
    )
    .is_related();
    let strict_uncached = query_relation(
        &interner,
        source,
        target,
        RelationKind::Subtype,
        strict_readonly,
        RelationContext::default(),
    )
    .is_related();

    assert!(
        ordinary_uncached,
        "ordinary structural relation should ignore property readonly",
    );
    assert!(
        !strict_uncached,
        "identity-style relation should treat property readonly as observable",
    );

    assert_eq!(
        db.is_subtype_of_with_policy(source, target, ordinary),
        ordinary_uncached,
        "cached ordinary readonly policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary readonly result must be stored in the ordinary slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(strict_key),
        None,
        "strict-readonly lookup must not hit the ordinary slot",
    );

    assert_eq!(
        db.is_subtype_of_with_policy(source, target, strict_readonly),
        strict_uncached,
        "cached strict-readonly policy must match direct query_relation",
    );
    assert_eq!(
        db.lookup_subtype_cache(strict_key),
        Some(strict_uncached),
        "strict-readonly result must be stored in the strict slot",
    );
    assert_eq!(
        db.lookup_subtype_cache(ordinary_key),
        Some(ordinary_uncached),
        "ordinary slot must remain intact after the strict-readonly lookup",
    );
}

// =============================================================================
// Packed relation-flag cache slot isolation
// =============================================================================

#[test]
fn disable_method_bivariance_slot_does_not_collide_with_bivariant_slot() {
    // `DISABLE_METHOD_BIVARIANCE` is packed into the relation flags and is
    // projected through `RelationPolicy`. Results computed with bivariance
    // enabled must not be served to checks with it disabled.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let lit = interner.literal_string("bivariance-isolation");

    let bivariant_config = RelationPolicy::unflagged_compatibility().cache_config();
    let strict_config =
        RelationPolicy::from_relation_flags(RelationFlags::DISABLE_METHOD_BIVARIANCE)
            .cache_config();

    let bivariant_key = RelationCacheKey::for_subtype(lit, TypeId::STRING, bivariant_config);
    let strict_key = RelationCacheKey::for_subtype(lit, TypeId::STRING, strict_config);

    assert_ne!(
        bivariant_key, strict_key,
        "bivariant and strict-bivariance keys must differ"
    );

    db.insert_subtype_cache(bivariant_key, true);

    assert_eq!(
        db.lookup_subtype_cache(strict_key),
        None,
        "strict-bivariance lookup must not hit the bivariant cache slot"
    );
    assert_eq!(
        db.lookup_subtype_cache(bivariant_key),
        Some(true),
        "bivariant slot must remain intact"
    );
}

//! Regression tests for `RelationCacheConfig` / `RelationCacheKey` behavior.
//!
//! These tests pin down the cache-partitioning contract:
//!
//! 1. Every behavior-affecting configuration change must produce a distinct
//!    [`RelationCacheKey`] so that results cannot accidentally share a slot.
//! 2. `RelationPolicy::from_flags` must NOT derive `strict_any_propagation`
//!    from `FLAG_STRICT_FUNCTION_TYPES` — those are independent compiler
//!    options.
//! 3. `skip_weak_type_checks` and `erase_generics` must partition cache
//!    entries (they actually change the relation outcome).
//! 4. Different `any_propagation_mode` values must produce distinct keys.

use super::*;
use crate::caches::db::QueryDatabase;
use crate::caches::query_cache::QueryCache;
use crate::intern::TypeInterner;
use crate::relations::relation_queries::RelationPolicy;
use crate::relations::subtype::AnyPropagationMode;
use crate::types::{
    CachedAnyMode, RelationCacheConfig, RelationCacheKey, RelationCacheKind, RelationFlags,
};

/// Assert that two `RelationPolicy` configurations produce distinct
/// assignability cache keys for the same `(STRING, NUMBER)` pair. Centralises
/// the build-two-keys / `assert_ne!` shape used by the per-flag partition
/// regression tests below.
fn assert_assignability_partitions(name: &str, on: RelationPolicy, off: RelationPolicy) {
    let key_on =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, on.cache_config());
    let key_off =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, off.cache_config());
    assert_ne!(key_on, key_off, "{name} must partition the cache");
}

/// Subtype-cache counterpart of [`assert_assignability_partitions`].
fn assert_subtype_partitions(name: &str, on: RelationPolicy, off: RelationPolicy) {
    let key_on = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, on.cache_config());
    let key_off = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, off.cache_config());
    assert_ne!(key_on, key_off, "{name} must partition the cache");
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
        RelationFlags::STRICT_SUBTYPE_CHECKING,
        RelationFlags::STRICT_ANY_PROPAGATION,
        RelationFlags::SKIP_WEAK_TYPE_CHECKS,
        RelationFlags::ASSUME_RELATED_ON_CYCLE,
    ];

    for bit in single_bits {
        let flipped = RelationCacheConfig::new(base.flags | bit, base.any_mode);
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
fn any_propagation_mode_differences_produce_distinct_keys() {
    let any_modes = [
        CachedAnyMode::All,
        CachedAnyMode::TopLevelOnlyAtTop,
        CachedAnyMode::TopLevelOnlyNested,
    ];
    for (i, &a) in any_modes.iter().enumerate() {
        for (j, &b) in any_modes.iter().enumerate() {
            let ka = RelationCacheKey::for_subtype(
                TypeId::STRING,
                TypeId::NUMBER,
                RelationCacheConfig::new(RelationFlags::empty(), a),
            );
            let kb = RelationCacheKey::for_subtype(
                TypeId::STRING,
                TypeId::NUMBER,
                RelationCacheConfig::new(RelationFlags::empty(), b),
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
fn erase_generics_partitions_cache_entries() {
    assert_subtype_partitions(
        "erase_generics",
        RelationPolicy::default().with_erase_generics(true),
        RelationPolicy::default().with_erase_generics(false),
    );
}

#[test]
fn strict_subtype_checking_partitions_cache_entries() {
    assert_assignability_partitions(
        "strict_subtype_checking",
        RelationPolicy::default().with_strict_subtype_checking(false),
        RelationPolicy::default().with_strict_subtype_checking(true),
    );
}

#[test]
fn strict_any_propagation_partitions_cache_entries() {
    assert_assignability_partitions(
        "strict_any_propagation",
        RelationPolicy::default().with_strict_any_propagation(false),
        RelationPolicy::default().with_strict_any_propagation(true),
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
fn any_propagation_mode_partitions_cache_entries_via_policy() {
    assert_subtype_partitions(
        "any_propagation_mode",
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::All),
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::TopLevelOnly),
    );
}

// =============================================================================
// 2. Regression: strict_function_types does NOT imply strict_any_propagation
// =============================================================================

#[test]
fn strict_function_types_does_not_imply_strict_any_propagation() {
    // Before the fix, `RelationPolicy::from_flags` inferred
    // `strict_any_propagation = true` whenever `FLAG_STRICT_FUNCTION_TYPES`
    // was set. Those are independent compiler options and must be tracked
    // separately; conflating them silently enabled Sound-Mode `any`
    // semantics in plain strict-function-types builds.
    let policy = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES);

    assert!(
        !policy.strict_any_propagation,
        "FLAG_STRICT_FUNCTION_TYPES must not imply strict_any_propagation",
    );
    assert_eq!(
        policy.any_propagation_mode,
        AnyPropagationMode::All,
        "FLAG_STRICT_FUNCTION_TYPES must not switch any_propagation_mode away from the default",
    );
}

#[test]
fn strict_function_types_and_strict_any_have_distinct_keys() {
    // Flipping only `strict_function_types` must NOT produce the same
    // cache key as flipping only `strict_any_propagation`. Before the fix
    // they were conflated, so the cache could serve the wrong result
    // depending on which came first.
    let sft_only =
        RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES).cache_config();
    let sap_only = RelationPolicy::default()
        .with_strict_any_propagation(true)
        .cache_config();

    assert_ne!(
        sft_only, sap_only,
        "strict_function_types and strict_any_propagation must produce different configs",
    );

    let k_sft = RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, sft_only);
    let k_sap = RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, sap_only);
    assert_ne!(
        k_sft, k_sap,
        "keys for strict_function_types and strict_any_propagation must be distinct",
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
fn policy_cache_config_preserves_packed_extended_bits() {
    let packed = (RelationFlags::STRICT_SUBTYPE_CHECKING
        | RelationFlags::STRICT_ANY_PROPAGATION
        | RelationFlags::SKIP_WEAK_TYPE_CHECKS
        | RelationFlags::ASSUME_RELATED_ON_CYCLE
        | RelationFlags::IN_CALLBACK_PARAM_CHECK
        | RelationFlags::STRICT_READONLY_IDENTITY)
        .bits() as u16;

    let config = RelationPolicy::from_flags(packed).cache_config();

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

    assert!(db.is_subtype_of_with_flags(source, TypeId::STRING, flags));
    assert_eq!(
        db.lookup_subtype_cache(RelationCacheKey::for_subtype(
            source,
            TypeId::STRING,
            config,
        )),
        Some(true),
        "subtype miss path must insert under the policy-derived cache key",
    );

    assert!(db.is_assignable_to_with_flags(source, TypeId::STRING, flags));
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

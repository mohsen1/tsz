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
use crate::relations::relation_queries::RelationPolicy;
use crate::relations::subtype::AnyPropagationMode;
use crate::types::{
    CachedAnyMode, RelationCacheConfig, RelationCacheKey, RelationCacheKind, RelationFlags,
};

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
    let with_weak = RelationPolicy::default().with_skip_weak_type_checks(false);
    let without_weak = RelationPolicy::default().with_skip_weak_type_checks(true);

    let key_on = RelationCacheKey::for_assignability(
        TypeId::STRING,
        TypeId::NUMBER,
        with_weak.cache_config(),
    );
    let key_off = RelationCacheKey::for_assignability(
        TypeId::STRING,
        TypeId::NUMBER,
        without_weak.cache_config(),
    );
    assert_ne!(
        key_on, key_off,
        "skip_weak_type_checks must partition the cache",
    );
}

#[test]
fn erase_generics_partitions_cache_entries() {
    let erase = RelationPolicy::default().with_erase_generics(true);
    let no_erase = RelationPolicy::default().with_erase_generics(false);

    let key_erase =
        RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, erase.cache_config());
    let key_no_erase =
        RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, no_erase.cache_config());

    assert_ne!(
        key_erase, key_no_erase,
        "erase_generics must partition the cache",
    );
}

#[test]
fn strict_subtype_checking_partitions_cache_entries() {
    let lax = RelationPolicy::default().with_strict_subtype_checking(false);
    let strict = RelationPolicy::default().with_strict_subtype_checking(true);

    let key_lax =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, lax.cache_config());
    let key_strict =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, strict.cache_config());

    assert_ne!(
        key_lax, key_strict,
        "strict_subtype_checking must partition the cache",
    );
}

#[test]
fn strict_any_propagation_partitions_cache_entries() {
    let lax = RelationPolicy::default().with_strict_any_propagation(false);
    let strict = RelationPolicy::default().with_strict_any_propagation(true);

    let key_lax =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, lax.cache_config());
    let key_strict =
        RelationCacheKey::for_assignability(TypeId::STRING, TypeId::NUMBER, strict.cache_config());

    assert_ne!(
        key_lax, key_strict,
        "strict_any_propagation must partition the cache",
    );
}

#[test]
fn assume_related_on_cycle_partitions_cache_entries() {
    let assume = RelationPolicy::default().with_assume_related_on_cycle(true);
    let no_assume = RelationPolicy::default().with_assume_related_on_cycle(false);

    let key_assume =
        RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, assume.cache_config());
    let key_no_assume =
        RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, no_assume.cache_config());

    assert_ne!(
        key_assume, key_no_assume,
        "assume_related_on_cycle must partition the cache",
    );
}

#[test]
fn any_propagation_mode_partitions_cache_entries_via_policy() {
    let all = RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::All);
    let top_only =
        RelationPolicy::default().with_any_propagation_mode(AnyPropagationMode::TopLevelOnly);

    let key_all = RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, all.cache_config());
    let key_top =
        RelationCacheKey::for_subtype(TypeId::STRING, TypeId::NUMBER, top_only.cache_config());

    assert_ne!(
        key_all, key_top,
        "any_propagation_mode must partition the cache",
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
// 3. Constructors route through the typed representation
// =============================================================================

#[test]
fn for_subtype_and_legacy_subtype_produce_equal_keys_for_same_config() {
    // The legacy `subtype(source, target, flags, any_mode)` constructor is
    // a compatibility shim. Callers still using the legacy protocol must
    // get exactly the same key as callers using the typed builder for the
    // same logical configuration.
    let flags =
        RelationCacheKey::FLAG_STRICT_NULL_CHECKS | RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES;
    let typed = RelationCacheKey::for_subtype(
        TypeId::STRING,
        TypeId::NUMBER,
        RelationCacheConfig::from_flags(RelationFlags::from_bits_truncate(flags as u32)),
    );
    let legacy = RelationCacheKey::subtype(TypeId::STRING, TypeId::NUMBER, flags, 0);

    assert_eq!(typed, legacy);
}

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

// =============================================================================
// 4. Hardening: lossy legacy helpers assert on invalid input in debug builds
// =============================================================================

#[test]
fn from_checker_flags_u16_preserves_every_known_bit_verbatim() {
    // The `u16`→typed bridge must be a pure widening for any bit the typed
    // API knows about; it must never silently strip a bit a caller set.
    let every_bit = RelationFlags::all();
    assert!(
        u32::from(u16::MAX) & every_bit.bits() == every_bit.bits(),
        "all known RelationFlags bits must fit in u16",
    );
    let packed = every_bit.bits() as u16;
    let config = RelationCacheConfig::from_checker_flags_u16(packed);
    assert_eq!(
        config.flags, every_bit,
        "from_checker_flags_u16 must preserve every known bit",
    );
    assert_eq!(
        config.any_mode,
        CachedAnyMode::All,
        "from_checker_flags_u16 must default any_mode to All",
    );
}

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "debug_assert only fires in debug builds"
)]
#[should_panic(expected = "RelationFlags layout")]
fn from_checker_flags_u16_panics_in_debug_on_unknown_bit() {
    // An unknown high bit is almost certainly a caller bug (they packed a
    // bit the typed API doesn't know about). We'd rather crash loudly in
    // debug than silently return a config that partitions the cache wrong.
    let stray = 1u16 << 15;
    let _ = RelationCacheConfig::from_checker_flags_u16(stray);
}

#[test]
#[cfg_attr(
    not(debug_assertions),
    ignore = "debug_assert only fires in debug builds"
)]
#[should_panic(expected = "out-of-range")]
fn cached_any_mode_from_legacy_u8_panics_in_debug_on_invalid_value() {
    // Only 0, 1, 2 are defined. Anything else would previously be silently
    // mapped to `TopLevelOnlyNested`. Debug builds now assert so tests
    // catch the miscoded caller.
    let _ = CachedAnyMode::from_legacy_u8(5);
}

#[test]
fn cached_any_mode_legacy_u8_roundtrip() {
    for mode in [
        CachedAnyMode::All,
        CachedAnyMode::TopLevelOnlyAtTop,
        CachedAnyMode::TopLevelOnlyNested,
    ] {
        assert_eq!(
            CachedAnyMode::from_legacy_u8(mode.to_legacy_u8()),
            mode,
            "legacy u8 round-trip must be lossless for every defined variant",
        );
    }
}

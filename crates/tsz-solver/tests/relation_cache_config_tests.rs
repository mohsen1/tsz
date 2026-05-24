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
//! 5. Every `RelationFlag` bit produces a distinct key, including
//!    `ALLOW_ERASED_GENERIC_SIGNATURE_RETRY`, `IN_CALLBACK_PARAM_CHECK`,
//!    and `STRICT_READONLY_IDENTITY`.
//! 6. Every sound-mode policy knob (`STRICT_ANY_PROPAGATION`,
//!    `STRICT_SUBTYPE_CHECKING`, `DISABLE_METHOD_BIVARIANCE`) must
//!    produce a distinct cache slot that does not collide with the
//!    corresponding non-sound slot.
//! 7. The `QueryCache` must not serve a non-sound cached result to a
//!    sound-mode lookup for the same type pair.
//! 8. Typed `RelationPolicy` query-cache entrypoints insert under
//!    policy-derived cache keys.
//! 9. The typed no-flags compatibility constructor remains equivalent to the
//!    legacy `RelationPolicy::from_flags(0)` constructor, without collapsing
//!    into `RelationPolicy::default()`.

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
        // Transient flags set during checker execution — they reach the cache
        // via packed `u16` flags rather than a typed builder field, but they
        // must still partition the cache to keep distinct relation passes in
        // separate slots.
        RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY,
        RelationFlags::IN_CALLBACK_PARAM_CHECK,
        RelationFlags::STRICT_READONLY_IDENTITY,
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
    assert_packed_flag_partitions(
        "in_callback_param_check",
        RelationFlags::IN_CALLBACK_PARAM_CHECK.bits() as u16,
    );
}

#[test]
fn strict_readonly_identity_partitions_cache_entries() {
    // Toggled during conditional-type distribution; results computed under
    // this mode must not share a slot with ordinary relation results.
    assert_packed_flag_partitions(
        "strict_readonly_identity",
        RelationFlags::STRICT_READONLY_IDENTITY.bits() as u16,
    );
}

// =============================================================================
// Sound-mode cache slot isolation
//
// These tests verify the end-to-end property described in SOUND_MODE.md §
// "The Caching Correctness Tax": a result cached under a non-sound policy
// must never be served to a sound-mode lookup for the same type pair.
//
// Rule for adding a new sound policy knob:
//   1. Add a field to `RelationPolicy` (or use the packed `flags` field if the
//      knob is set transiently inside the checker).
//   2. Map it to a `RelationFlags` bit and reflect it in `cache_config()`.
//   3. Add a `*_partitions_cache_entries` test (see the section above).
//   4. Add a `*_slot_does_not_collide_with_non_sound_slot` isolation test
//      (mirror the pattern below) to prove non-sound results cannot
//      contaminate sound-mode lookups.
// =============================================================================

#[test]
fn strict_any_propagation_slot_does_not_collide_with_non_sound_slot() {
    // Prove that a result cached in the non-sound slot (no STRICT_ANY_PROPAGATION)
    // cannot be retrieved via a sound-mode lookup key (with STRICT_ANY_PROPAGATION).
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);
    let lit = interner.literal_string("hello");

    let non_sound_config =
        RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS).cache_config();
    let sound_config = RelationPolicy::from_flags(RelationCacheKey::FLAG_STRICT_NULL_CHECKS)
        .with_strict_any_propagation(true)
        .cache_config();

    let non_sound_key = RelationCacheKey::for_subtype(lit, TypeId::STRING, non_sound_config);
    let sound_key = RelationCacheKey::for_subtype(lit, TypeId::STRING, sound_config);

    assert_ne!(
        non_sound_key, sound_key,
        "non-sound and sound keys must differ for STRICT_ANY_PROPAGATION"
    );

    db.insert_subtype_cache(non_sound_key, true);

    assert_eq!(
        db.lookup_subtype_cache(sound_key),
        None,
        "sound-mode lookup must not hit the non-sound cache slot"
    );
    assert_eq!(
        db.lookup_subtype_cache(non_sound_key),
        Some(true),
        "non-sound slot must remain intact after a sound-mode miss"
    );
}

#[test]
fn strict_subtype_checking_slot_does_not_collide_with_non_sound_slot() {
    // `STRICT_SUBTYPE_CHECKING` is the sound-mode flag that implies method
    // bivariance disablement inside `CompatChecker`. Results cached under
    // this policy must not be served to non-sound lookups.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let lit = interner.literal_string("sound-checker-isolation");

    let non_sound_config = RelationPolicy::from_flags(0).cache_config();
    let sound_config = RelationPolicy::from_flags(0)
        .with_strict_subtype_checking(true)
        .cache_config();

    let non_sound_key = RelationCacheKey::for_assignability(lit, TypeId::STRING, non_sound_config);
    let sound_key = RelationCacheKey::for_assignability(lit, TypeId::STRING, sound_config);

    assert_ne!(
        non_sound_key, sound_key,
        "non-sound and sound assignability keys must differ for STRICT_SUBTYPE_CHECKING"
    );

    db.insert_assignability_cache(non_sound_key, true);

    assert_eq!(
        db.lookup_assignability_cache(sound_key),
        None,
        "sound-mode assignability lookup must not hit the non-sound slot"
    );
    assert_eq!(
        db.lookup_assignability_cache(non_sound_key),
        Some(true),
        "non-sound assignability slot must remain intact"
    );
}

#[test]
fn disable_method_bivariance_slot_does_not_collide_with_bivariant_slot() {
    // `DISABLE_METHOD_BIVARIANCE` is packed into the relation flags and is
    // projected through `RelationPolicy`. Results computed with bivariance
    // enabled must not be served to checks with it disabled.
    let interner = TypeInterner::new();
    let db = QueryCache::new(&interner);

    let lit = interner.literal_string("bivariance-isolation");

    let bivariant_config = RelationPolicy::from_flags(0).cache_config();
    let strict_config =
        RelationPolicy::from_flags(RelationCacheKey::FLAG_DISABLE_METHOD_BIVARIANCE).cache_config();

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

#[test]
fn canonical_sound_mode_policy_cache_key_contains_expected_flags() {
    // Prove that the canonical sound-mode `RelationPolicy` (as built by the
    // checker query boundary for every assignability check in sound mode)
    // encodes `STRICT_SUBTYPE_CHECKING` and `STRICT_ANY_PROPAGATION` in its
    // cache key. Any future sound-mode policy knob must appear here too.
    let sound_policy = RelationPolicy::default()
        .with_strict_subtype_checking(true)
        .with_strict_any_propagation(true);

    let config = sound_policy.cache_config();

    assert!(
        config
            .flags
            .contains(RelationFlags::STRICT_SUBTYPE_CHECKING),
        "sound mode policy cache key must include STRICT_SUBTYPE_CHECKING"
    );
    assert!(
        config.flags.contains(RelationFlags::STRICT_ANY_PROPAGATION),
        "sound mode policy cache key must include STRICT_ANY_PROPAGATION"
    );

    let default_config = RelationPolicy::default().cache_config();
    assert_ne!(
        config, default_config,
        "sound mode cache config must differ from the default non-sound config"
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
fn policy_cache_config_preserves_all_assigned_packed_bits() {
    let all_flags = RelationFlags::all();
    let config = RelationPolicy::from_flags(all_flags.bits() as u16).cache_config();

    assert_eq!(
        config.flags, all_flags,
        "explicit packed-bit projection must preserve every assigned relation flag",
    );
}

#[test]
fn relation_policy_typed_accessors_preserve_packed_relation_bits() {
    let packed = RelationCacheKey::FLAG_STRICT_NULL_CHECKS
        | RelationCacheKey::FLAG_STRICT_FUNCTION_TYPES
        | RelationCacheKey::FLAG_EXACT_OPTIONAL_PROPERTY_TYPES
        | RelationCacheKey::FLAG_NO_UNCHECKED_INDEXED_ACCESS
        | RelationCacheKey::FLAG_DISABLE_METHOD_BIVARIANCE
        | RelationCacheKey::FLAG_ALLOW_VOID_RETURN
        | RelationCacheKey::FLAG_ALLOW_BIVARIANT_REST
        | RelationCacheKey::FLAG_ALLOW_BIVARIANT_PARAM_COUNT
        | RelationCacheKey::FLAG_ALLOW_ERASED_GENERIC_SIGNATURE_RETRY
        | RelationFlags::STRICT_READONLY_IDENTITY.bits() as u16;
    let enabled = RelationPolicy::from_flags(packed);
    let disabled = RelationPolicy::from_flags(0);

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

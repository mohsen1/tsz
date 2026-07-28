//! Subtype-checker cache policy projection tests.

use crate::intern::TypeInterner;
use crate::relations::relation_queries::RelationPolicy;
use crate::relations::subtype::{AnyPropagationMode, SubtypeChecker};
use crate::types::{CachedAnyMode, RelationCacheKey, RelationFlags, TypeId};

#[test]
fn subtype_cache_key_uses_named_policy_projection_helpers() {
    let source = include_str!("../../src/relations/subtype/helpers.rs");
    let make_cache_key = source
        .split_once("pub(crate) fn make_cache_key")
        .and_then(|(_, rest)| rest.split_once("/// Project this"))
        .map(|(body, _)| body)
        .expect("expected `SubtypeChecker::make_cache_key` before `cache_policy` helper");

    assert!(
        source.contains("fn cache_policy(") && make_cache_key.contains("self.cache_policy()"),
        "`SubtypeChecker::make_cache_key` should delegate policy construction to a named helper",
    );
    assert!(
        source.contains("fn effective_cached_any_mode(")
            && make_cache_key.contains("self.effective_cached_any_mode()"),
        "`SubtypeChecker::make_cache_key` should delegate depth-sensitive any-mode projection",
    );
}

#[test]
fn subtype_cache_key_matches_equivalent_relation_policy_projection() {
    let interner = TypeInterner::new();
    let source = TypeId::STRING;
    let target = TypeId::NUMBER;
    let mut checker = SubtypeChecker::new(&interner)
        .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly)
        .with_assume_related_on_cycle(false)
        .with_assume_related_on_depth(false);
    checker.strict_null_checks = true;
    checker.strict_function_types = true;
    checker.exact_optional_property_types = true;
    checker.strict_readonly_identity = true;
    checker.no_unchecked_indexed_access = true;
    checker.disable_method_bivariance = true;
    checker.allow_void_return = true;
    checker.allow_bivariant_rest = true;
    checker.allow_bivariant_param_count = true;
    checker.erase_generics = false;
    checker.allow_erased_generic_signature_retry = true;
    checker.in_callback_param_check = true;

    let expected_flags = RelationFlags::STRICT_NULL_CHECKS
        | RelationFlags::STRICT_FUNCTION_TYPES
        | RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES
        | RelationFlags::NO_UNCHECKED_INDEXED_ACCESS
        | RelationFlags::DISABLE_METHOD_BIVARIANCE
        | RelationFlags::ALLOW_VOID_RETURN
        | RelationFlags::ALLOW_BIVARIANT_REST
        | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT
        | RelationFlags::NO_ERASE_GENERICS
        | RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY
        | RelationFlags::IN_CALLBACK_PARAM_CHECK
        | RelationFlags::STRICT_READONLY_IDENTITY;
    let expected_policy = RelationPolicy::from_relation_flags(expected_flags)
        .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly)
        .with_assume_related_on_cycle(false)
        .with_assume_related_on_depth(false);

    let key = checker.debug_cache_key_for(source, target);

    assert_eq!(
        key,
        RelationCacheKey::for_subtype(
            source,
            target,
            expected_policy.cache_config_with_cached_any_mode(CachedAnyMode::TopLevelOnlyAtTop),
        ),
        "equivalent `SubtypeChecker` and `RelationPolicy` configurations must share one cache key",
    );
}

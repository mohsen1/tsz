//! Cache-config projection tests for depth-resolved `any` modes.

use crate::relations::relation_queries::RelationPolicy;
use crate::relations::subtype::AnyPropagationMode;
use crate::types::{CachedAnyMode, RelationFlags};

#[test]
fn effective_cached_any_mode_projection_preserves_policy_bits() {
    let passthrough_flags = RelationFlags::STRICT_NULL_CHECKS
        | RelationFlags::STRICT_FUNCTION_TYPES
        | RelationFlags::EXACT_OPTIONAL_PROPERTY_TYPES
        | RelationFlags::NO_UNCHECKED_INDEXED_ACCESS
        | RelationFlags::DISABLE_METHOD_BIVARIANCE
        | RelationFlags::ALLOW_VOID_RETURN
        | RelationFlags::ALLOW_BIVARIANT_REST
        | RelationFlags::ALLOW_BIVARIANT_PARAM_COUNT
        | RelationFlags::ALLOW_ERASED_GENERIC_SIGNATURE_RETRY
        | RelationFlags::IN_CALLBACK_PARAM_CHECK
        | RelationFlags::STRICT_READONLY_IDENTITY;
    let policy = RelationPolicy::from_relation_flags(passthrough_flags)
        .with_strict_subtype_checking(true)
        .with_strict_any_propagation(true)
        .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly)
        .with_assume_related_on_cycle(true)
        .with_skip_weak_type_checks(true)
        .with_erase_generics(false);
    let expected_flags = passthrough_flags
        | RelationFlags::STRICT_SUBTYPE_CHECKING
        | RelationFlags::STRICT_ANY_PROPAGATION
        | RelationFlags::ASSUME_RELATED_ON_CYCLE
        | RelationFlags::ASSUME_RELATED_ON_DEPTH
        | RelationFlags::SKIP_WEAK_TYPE_CHECKS
        | RelationFlags::NO_ERASE_GENERICS;

    let all_config = policy.cache_config_with_cached_any_mode(CachedAnyMode::All);
    let top_config = policy.cache_config_with_cached_any_mode(CachedAnyMode::TopLevelOnlyAtTop);
    let nested_config = policy.cache_config_with_cached_any_mode(CachedAnyMode::TopLevelOnlyNested);

    assert_eq!(
        all_config.flags, expected_flags,
        "resolved `any` mode projection must preserve all policy bits for `All`",
    );
    assert_eq!(
        top_config.flags, expected_flags,
        "resolved `any` mode projection must preserve all policy bits at top level",
    );
    assert_eq!(
        nested_config.flags, expected_flags,
        "resolved `any` mode projection must preserve all policy bits when nested",
    );
    assert_eq!(all_config.any_mode, CachedAnyMode::All);
    assert_eq!(top_config.any_mode, CachedAnyMode::TopLevelOnlyAtTop);
    assert_eq!(nested_config.any_mode, CachedAnyMode::TopLevelOnlyNested);
}

#[test]
fn effective_cached_any_mode_projection_honors_builder_overrides() {
    let passthrough_flags =
        RelationFlags::STRICT_NULL_CHECKS | RelationFlags::STRICT_FUNCTION_TYPES;
    let stale_field_owned_flags = RelationFlags::STRICT_SUBTYPE_CHECKING
        | RelationFlags::STRICT_ANY_PROPAGATION
        | RelationFlags::ASSUME_RELATED_ON_CYCLE
        | RelationFlags::SKIP_WEAK_TYPE_CHECKS
        | RelationFlags::NO_ERASE_GENERICS;
    let policy = RelationPolicy::from_relation_flags(passthrough_flags | stale_field_owned_flags)
        .with_strict_subtype_checking(false)
        .with_strict_any_propagation(false)
        .with_any_propagation_mode(AnyPropagationMode::TopLevelOnly)
        .with_assume_related_on_cycle(false)
        .with_assume_related_on_depth(false)
        .with_skip_weak_type_checks(false)
        .with_erase_generics(true);

    for any_mode in [
        CachedAnyMode::All,
        CachedAnyMode::TopLevelOnlyAtTop,
        CachedAnyMode::TopLevelOnlyNested,
    ] {
        let config = policy.cache_config_with_cached_any_mode(any_mode);
        assert_eq!(
            config.flags, passthrough_flags,
            "builder-owned fields must override stale flag bits for {any_mode:?}",
        );
        assert_eq!(
            config.any_mode, any_mode,
            "resolved `any` mode must be the only projection-local difference",
        );
    }
}

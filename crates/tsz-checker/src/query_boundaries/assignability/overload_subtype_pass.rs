//! Overload-resolution subtype-pass assignability boundary (issue #13042).
//!
//! Submodule keeps the assignability boundary under its LOC ceiling while the
//! boundary still owns the cache-key construction and relation execution.

use super::relation_policy;
use super::{
    AssignabilityQueryInputs, RelationContext, RelationPolicy, RelationQueryInputs,
    SolverRelationKind, assignability_cache_key_for_policy, is_relation_cacheable,
};

pub(super) fn is_assignable_with_policy_and_overrides<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    policy: RelationPolicy,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let _span = tracing::debug_span!(
        "is_assignable",
        src = inputs.source.0,
        tgt = inputs.target.0,
    )
    .entered();

    let AssignabilityQueryInputs {
        db,
        resolver,
        source,
        target,
        inheritance_graph,
        evaluation_session,
        ..
    } = *inputs;
    let context = RelationContext {
        query_db: Some(db),
        evaluation_session,
        inheritance_graph: Some(inheritance_graph),
        class_check: None,
    };
    tsz_solver::relations::relation_queries::query_relation_with_overrides(RelationQueryInputs {
        interner: db.as_type_database(),
        resolver,
        source,
        target,
        kind: SolverRelationKind::Assignable,
        policy,
        context,
        overrides,
    })
}

/// Execute an assignability relation through the boundary-owned checker cache.
///
/// Checker callers provide prepared relation inputs and an exact typed policy;
/// cacheability, key construction, execution, and insertion remain together so
/// a non-default policy can never read or write the default relation slot.
pub(super) fn cached_assignability_with_policy_and_overrides<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    policy: RelationPolicy,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let is_cacheable =
        is_relation_cacheable(inputs.db.as_type_database(), inputs.source, inputs.target);
    let cache_key = assignability_cache_key_for_policy(
        inputs.source,
        inputs.target,
        policy,
        inputs.inheritance_graph,
    );
    if is_cacheable && let Some(cached) = inputs.db.lookup_assignability_cache(cache_key) {
        return tsz_solver::relations::relation_queries::RelationResult::complete(
            tsz_solver::relations::relation_queries::RelationKind::Assignable,
            cached,
        );
    }

    let relation_result = is_assignable_with_policy_and_overrides(inputs, policy, overrides);

    if is_cacheable && relation_result.is_cacheable() {
        inputs
            .db
            .insert_assignability_cache(cache_key, relation_result.is_related());
    }

    relation_result
}

/// Overload-resolution subtype pass (tsc `chooseOverload` with
/// `subtypeRelation`): assignability where an `any` source is not related to
/// non-`any`/`unknown` targets at every nesting level, while an `any` target
/// still accepts everything.
///
/// The pass rides on the typed `AnySourceNotRelated` propagation mode rather
/// than the packed `u16` flag protocol (which is saturated). The mode is part
/// of `RelationPolicy::cache_config`, so the checker-level cache key built
/// here can never share a slot with default assignable-relation results.
pub(crate) fn cached_overload_subtype_pass_assignability<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let policy = relation_policy::from_checker_flags_u16(inputs.flags)
        .with_strict_subtype_checking(inputs.sound_mode)
        .with_strict_any_propagation(inputs.sound_mode)
        .with_any_propagation_mode(
            tsz_solver::relations::subtype::AnyPropagationMode::AnySourceNotRelated,
        );
    cached_assignability_with_policy_and_overrides(inputs, policy, overrides)
}

/// Strict generic-call aggregate-rest assignability.
///
/// The provisional-rest policy is a typed bit beyond the checker's legacy
/// packed `u16` mask. The overload subtype pass remains an independent typed
/// policy dimension so its `any` behavior and cache entries are preserved.
pub(crate) fn cached_provisional_rest_union_assignability<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let policy = relation_policy::from_checker_flags_u16(inputs.flags)
        .with_strict_subtype_checking(inputs.sound_mode)
        .with_strict_any_propagation(inputs.sound_mode)
        .with_provisional_rest_union(true);
    cached_assignability_with_policy_and_overrides(inputs, policy, overrides)
}

pub(crate) fn cached_overload_subtype_pass_provisional_rest_union_assignability<
    R: tsz_solver::relations::subtype::TypeResolver,
>(
    inputs: &AssignabilityQueryInputs<'_, R>,
    overrides: &dyn tsz_solver::relations::compat::AssignabilityOverrideProvider,
) -> tsz_solver::relations::relation_queries::RelationResult {
    let policy = relation_policy::from_checker_flags_u16(inputs.flags)
        .with_strict_subtype_checking(inputs.sound_mode)
        .with_strict_any_propagation(inputs.sound_mode)
        .with_provisional_rest_union(true)
        .with_any_propagation_mode(
            tsz_solver::relations::subtype::AnyPropagationMode::AnySourceNotRelated,
        );
    cached_assignability_with_policy_and_overrides(inputs, policy, overrides)
}

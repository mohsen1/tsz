//! Overload-resolution subtype-pass assignability boundary (issue #13042).
//!
//! Submodule keeps the assignability boundary under its LOC ceiling while the
//! boundary still owns the cache-key construction and relation execution.

use super::relation_policy;
use super::{
    AssignabilityQueryInputs, RelationQueryInputs, SolverRelationKind,
    assignability_cache_key_for_policy, is_relation_cacheable,
};

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

    let context = tsz_solver::relations::relation_queries::RelationContext {
        query_db: Some(inputs.db),
        evaluation_session: inputs.evaluation_session,
        inheritance_graph: Some(inputs.inheritance_graph),
        class_check: None,
    };
    let relation_result = tsz_solver::relations::relation_queries::query_relation_with_overrides(
        RelationQueryInputs {
            interner: inputs.db.as_type_database(),
            resolver: inputs.resolver,
            source: inputs.source,
            target: inputs.target,
            kind: SolverRelationKind::Assignable,
            policy,
            context,
            overrides,
        },
    );

    if is_cacheable {
        inputs
            .db
            .insert_assignability_cache(cache_key, relation_result.is_related());
    }

    relation_result
}

//! Checker-final assignability funnel (issue #13243 step 4).
//!
//! Owns the *final* assignability verdict for the checker's default
//! `is_assignable_to` / `is_assignable_to_with_env` gateways: relation
//! execution plus the checker's post-relation true-override gates
//! (alias-application argument rejection, iterator-protocol mismatch,
//! namespace property mismatch, keyof literal membership), cached under the
//! dedicated [`tsz_solver::RelationCacheKind::CheckerAssignable`] key kind.
//!
//! Cache honesty contract: a cached verdict here is authoritative — callers
//! return it without any post-processing. Raw Lawyer-relation entries
//! (`RelationCacheKind::Assignable`) live in a disjoint key namespace, so
//! solver-internal relation caching and the checker-final verdict can never
//! poison each other.

use tracing::trace;
use tsz_solver::TypeId;

use super::{
    AssignabilityQueryInputs, checker_final_assignability_cache_key, is_assignable_with_overrides,
    is_relation_cacheable,
};
use crate::state::{CheckerOverrideProvider, CheckerState};

/// Execute the checker-final assignability relation for prepared (evaluated)
/// source/target types: cache lookup → relation execution → post-relation
/// true-override gates → cache insert.
///
/// The relation result is inserted provisionally *before* the gates run so
/// recursive relation queries issued by the gates observe the raw relation
/// verdict (the pre-#13243 post-pass semantics); a gate rejection then
/// downgrades the stored entry to the final `false`.
///
/// `use_env_resolver` selects the `TypeEnvironment` resolver used by generic
/// call/new inference (`is_assignable_to_with_env`) instead of the checker
/// context resolver.
pub(crate) fn cached_final_assignability(
    checker: &mut CheckerState<'_>,
    source: TypeId,
    target: TypeId,
    use_env_resolver: bool,
    label: &str,
) -> bool {
    let flags = checker.ctx.pack_relation_flags();
    let is_cacheable = is_relation_cacheable(checker.ctx.types.as_type_database(), source, target);
    let cache_key = checker_final_assignability_cache_key(
        source,
        target,
        flags,
        &checker.ctx.inheritance_graph,
    );
    if is_cacheable && let Some(cached) = checker.ctx.types.lookup_assignability_cache(cache_key) {
        trace!(
            source = source.0,
            target = target.0,
            result = cached,
            cached = true,
            "{label}"
        );
        return cached;
    }

    let relation_result = if use_env_resolver {
        let env = checker.ctx.type_env.borrow();
        let overrides = CheckerOverrideProvider::new(checker, Some(&env));
        is_assignable_with_overrides(
            &AssignabilityQueryInputs {
                db: checker.ctx.types,
                resolver: &*env,
                source,
                target,
                flags,
                inheritance_graph: &checker.ctx.inheritance_graph,
                sound_mode: checker.ctx.sound_mode(),
                evaluation_session: Some(checker.ctx.eval_session.as_ref()),
            },
            &overrides,
        )
    } else {
        let overrides = CheckerOverrideProvider::new(checker, None);
        is_assignable_with_overrides(
            &AssignabilityQueryInputs {
                db: checker.ctx.types,
                resolver: &checker.ctx,
                source,
                target,
                flags,
                inheritance_graph: &checker.ctx.inheritance_graph,
                sound_mode: checker.ctx.sound_mode(),
                evaluation_session: Some(checker.ctx.eval_session.as_ref()),
            },
            &overrides,
        )
    };
    let raw_related = relation_result.is_related();
    checker.propagate_overflow_flags(
        relation_result.depth_exceeded(),
        relation_result.iteration_exceeded(),
    );

    if is_cacheable {
        checker
            .ctx
            .types
            .insert_assignability_cache(cache_key, raw_related);
    }

    let result = raw_related && !checker.assignability_true_override_rejects(source, target);
    if is_cacheable && result != raw_related {
        checker
            .ctx
            .types
            .insert_assignability_cache(cache_key, result);
    }

    trace!(
        source = source.0,
        target = target.0,
        result,
        cached = false,
        "{label}"
    );
    result
}

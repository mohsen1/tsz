//! Indexed-access helpers for generic inference constraints.

use crate::inference::infer::InferenceContext;
use crate::operations::{AssignabilityChecker, CallEvaluator};
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashMap;

impl<'a, C: AssignabilityChecker> CallEvaluator<'a, C> {
    /// Reduce an indexed-access type `object[index]` to its member type during
    /// inference constraint collection.
    ///
    /// The bare-interner reduction (`evaluate_index_access`) cannot resolve an
    /// object that is itself a `Lazy`/`Application` of a generic type, because it
    /// runs without a resolver: `Ord<A>['compare']` (a parameter type during a
    /// generic call) is left unevaluated, so the access never exposes its member
    /// type and no candidate is collected for `A`, which then collapses to its
    /// default (`unknown`) — the inference half of #14261.
    ///
    /// When the bare reduction makes no progress, expand the object through the
    /// checker's resolver via [`AssignabilityChecker::expand_type_alias_application`],
    /// which instantiates the generic body while *preserving* inference
    /// placeholders (rather than collapsing them to their constraints, as a full
    /// evaluation would), and re-index the expanded object. For `Ord<A>` this
    /// yields `{ compare: (first: A, second: A) => Ordering }['compare']`, which
    /// the bare reducer then resolves to `(first: A, second: A) => Ordering`,
    /// exposing the inference site for `A`. Returns `original` unchanged when no
    /// reduction is possible, matching the prior no-progress contract callers
    /// already guard with `evaluated != original`.
    pub(super) fn reduce_index_access_for_inference(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        original: TypeId,
        object_type: TypeId,
        index_type: TypeId,
    ) -> TypeId {
        if let Some(reduced) =
            self.reduce_index_access_with_candidate_keys(ctx, var_map, object_type, index_type)
        {
            return reduced;
        }
        if let Some(expanded_object) = self.expanded_index_access_object(object_type)
            && expanded_object != object_type
            && let Some(reduced) = self.reduce_index_access_with_candidate_keys(
                ctx,
                var_map,
                expanded_object,
                index_type,
            )
        {
            return reduced;
        }

        let evaluated = self.interner.evaluate_index_access(object_type, index_type);
        if evaluated != original {
            return evaluated;
        }
        if let Some(expanded_object) = self.expanded_index_access_object(object_type)
            && expanded_object != object_type
        {
            let evaluated = self
                .interner
                .evaluate_index_access(expanded_object, index_type);
            if evaluated != original {
                return evaluated;
            }
        }
        // No reduction was possible; return the unchanged indexed access (which
        // equals `evaluated` here) so the caller's `!= original` guard reads it
        // as no progress.
        original
    }

    fn expanded_index_access_object(&mut self, object_type: TypeId) -> Option<TypeId> {
        self.checker.expand_type_alias_application(object_type)
    }

    fn reduce_index_access_with_candidate_keys(
        &mut self,
        ctx: &mut InferenceContext,
        var_map: &FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        let index_var = *var_map.get(&index_type)?;
        let constraints = ctx.get_constraints(index_var)?;
        let mut results = Vec::new();
        for key in constraints.lower_bounds {
            let Some(index_key) = self.literal_candidate_index_key(key) else {
                continue;
            };
            let reduced = self.interner.evaluate_index_access(object_type, index_key);
            if matches!(
                self.interner.lookup(reduced),
                Some(TypeData::IndexAccess(_, _))
            ) {
                continue;
            }
            if !results.contains(&reduced) {
                results.push(reduced);
            }
        }
        match results.len() {
            0 => None,
            1 => Some(results[0]),
            _ => Some(self.interner.union(results)),
        }
    }

    fn literal_candidate_index_key(&self, key: TypeId) -> Option<TypeId> {
        if crate::type_queries::extended::get_literal_property_name(
            self.interner.as_type_database(),
            key,
        )
        .is_some()
        {
            return Some(key);
        }
        let Some(TypeData::Union(members_id)) = self.interner.lookup(key) else {
            return None;
        };
        let members = self.interner.type_list(members_id);
        let literal_members: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|&member| {
                crate::type_queries::extended::get_literal_property_name(
                    self.interner.as_type_database(),
                    member,
                )
                .is_some()
            })
            .collect();
        match literal_members.len() {
            0 => None,
            1 => Some(literal_members[0]),
            _ => Some(self.interner.union(literal_members)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caches::query_cache::QueryCache;
    use crate::intern::TypeInterner;
    use crate::types::{InferencePriority, PropertyInfo, TypeParamInfo};

    struct NoopChecker;

    impl AssignabilityChecker for NoopChecker {
        fn is_assignable_to(&mut self, _source: TypeId, _target: TypeId) -> bool {
            false
        }
    }

    fn make_type_param(
        interner: &TypeInterner,
        name: &str,
    ) -> (tsz_common::interner::Atom, TypeId) {
        let atom = interner.intern_string(name);
        let ty = interner.type_param(TypeParamInfo::simple(atom));
        (atom, ty)
    }

    fn keyed_registry_fixture<'a>(
        interner: &'a TypeInterner,
    ) -> (
        InferenceContext<'a>,
        FxHashMap<TypeId, crate::inference::infer::InferenceVar>,
        crate::inference::infer::InferenceVar,
        crate::inference::infer::InferenceVar,
        TypeId,
        TypeId,
    ) {
        let mut ctx = InferenceContext::new(interner);
        let (key_name, key_type) = make_type_param(interner, "Key");
        let (payload_name, payload_type) = make_type_param(interner, "Payload");
        let key_var = ctx.fresh_type_param(key_name, false);
        let payload_var = ctx.fresh_type_param(payload_name, false);
        let mut var_map = FxHashMap::default();
        var_map.insert(key_type, key_var);
        var_map.insert(payload_type, payload_var);

        let item_atom = interner.intern_string("item");
        let boxed_atom = interner.intern_string("Boxed");
        let boxed_member = interner.object(vec![PropertyInfo::new(item_atom, payload_type)]);
        let registry = interner.object(vec![PropertyInfo::new(boxed_atom, boxed_member)]);
        let target = interner.index_access(registry, key_type);
        let source = interner.object(vec![PropertyInfo::new(item_atom, TypeId::NUMBER)]);

        (ctx, var_map, key_var, payload_var, source, target)
    }

    #[test]
    fn candidate_keyed_index_access_infers_selected_member_template() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        ctx.add_candidate(
            key_var,
            interner.literal_string("Boxed"),
            InferencePriority::NakedTypeVariable,
        );

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            ctx.resolve_with_constraints(payload_var).unwrap(),
            TypeId::NUMBER
        );
    }

    #[test]
    fn candidate_keyed_index_access_waits_for_key_evidence() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, _key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert!(!ctx.var_has_candidates(payload_var));
    }

    #[test]
    fn candidate_keyed_index_access_accepts_union_key_evidence() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        let key_union = interner.union(vec![
            interner.literal_string("Boxed"),
            interner.literal_string("Missing"),
        ]);
        ctx.add_candidate(key_var, key_union, InferencePriority::NakedTypeVariable);

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert_eq!(
            ctx.resolve_with_constraints(payload_var).unwrap(),
            TypeId::NUMBER
        );
    }

    #[test]
    fn candidate_keyed_index_access_ignores_missing_key_evidence() {
        let interner = TypeInterner::new();
        let cache = QueryCache::new(&interner);
        let mut checker = NoopChecker;
        let mut evaluator = CallEvaluator::new(&cache, &mut checker);
        let (mut ctx, var_map, key_var, payload_var, source, target) =
            keyed_registry_fixture(&interner);

        ctx.add_candidate(
            key_var,
            interner.literal_string("Missing"),
            InferencePriority::NakedTypeVariable,
        );

        evaluator.constrain_types(
            &mut ctx,
            &var_map,
            source,
            target,
            InferencePriority::NakedTypeVariable,
        );

        assert!(!ctx.var_has_candidates(payload_var));
    }
}

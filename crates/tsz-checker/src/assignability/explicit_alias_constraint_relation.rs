use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Execute an explicit alias constraint relation for raw checker types,
    /// preserving the canonical explicit-alias request shape. Decision-only:
    /// the sole consumer reads `outcome.related`, so failure analysis is
    /// skipped.
    ///
    /// Mirrors `type_arg_constraint_relation_outcome`'s success-cache tier
    /// (`relation_outcome_helpers.rs`): unlike its sibling constraint-relation
    /// helpers, this path sits in the mutually-generic-alias re-entry cycle
    /// (`explicit_alias_type_parameter_constraint_satisfies_arg_constraint` ->
    /// `explicit_alias_constraint_relation_outcome` ->
    /// `compute_type_of_symbol_type_alias_variable_alias`), so every call at a
    /// given nesting level re-proves the same relation from scratch without a
    /// success cache (#15729).
    pub(crate) fn explicit_alias_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        const RELATED_SUCCESS: crate::query_boundaries::assignability::RelationOutcome =
            crate::query_boundaries::assignability::RelationOutcome {
                related: true,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            };

        let (source, target) = self.prepare_assignability_inputs(source, target);
        let flags = self.ctx.pack_relation_flags();
        let sound_mode = self.ctx.sound_mode();
        let cache_key = (source, target, flags, sound_mode);
        if self
            .ctx
            .type_reference_validation_caches
            .explicit_alias_constraint_relation_successes
            .contains(&cache_key)
        {
            return RELATED_SUCCESS;
        }

        // Program-wide success tier: another file checker may already have
        // proven this exact pair.
        if self.shared_constraint_proof_hit(|s| {
            s.explicit_alias_relation_successes.contains(&cache_key)
        }) {
            tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "explicit_alias", "hit");
            self.ctx
                .type_reference_validation_caches
                .explicit_alias_constraint_relation_successes
                .insert(cache_key);
            return RELATED_SUCCESS;
        }

        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        let request =
            crate::query_boundaries::assignability::RelationRequest::explicit_alias_constraint(
                source, target,
            )
            .with_decision_only();
        let outcome = self.execute_relation_request(&request);
        if outcome.related && !outcome.depth_exceeded && !outcome.iteration_exceeded {
            self.ctx
                .type_reference_validation_caches
                .explicit_alias_constraint_relation_successes
                .insert(cache_key);
            self.publish_shared_constraint_proof(lazy_failures_at_entry, source, target, |shared| {
                tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "explicit_alias", "publish");
                shared.explicit_alias_relation_successes.insert(cache_key);
            });
        }
        outcome
    }
}

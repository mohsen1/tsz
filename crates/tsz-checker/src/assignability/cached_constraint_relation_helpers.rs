//! TS2344 constraint-relation outcomes that participate in the file-local and
//! program-wide [`crate::context::SharedConstraintProofCache`] success-cache
//! tier, split out of `relation_outcome_helpers.rs` to keep that file under
//! the checker's 2000-line boundary cap.

use crate::context::GenericConstraintProofKey;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Whether a constraint proof over `(source, target)` is file-independent
    /// and may be published to the program-wide
    /// [`crate::context::SharedConstraintProofCache`].
    ///
    /// Both types must be free of generic type parameters (scope-relative
    /// meaning) and of file-relative content (`UnresolvedTypeName`, raw
    /// `SymbolRef` carriers, `this`); see
    /// `contains_file_relative_content` for the exact variant set. Both
    /// predicates are memoized project-wide in the interner.
    fn constraint_proof_is_program_shareable(&self, source: TypeId, target: TypeId) -> bool {
        use crate::query_boundaries::common::{
            contains_file_relative_content, contains_generic_type_parameters,
        };
        let db = self.ctx.types;
        !contains_generic_type_parameters(db, source)
            && !contains_generic_type_parameters(db, target)
            && !contains_file_relative_content(db, source)
            && !contains_file_relative_content(db, target)
    }

    /// Typed checker-cache key for TS2344 constraint proof helpers that run
    /// relation/evaluation work under the current checker policy.
    pub(crate) const fn generic_constraint_proof_key(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> GenericConstraintProofKey {
        GenericConstraintProofKey::new(
            source,
            target,
            self.ctx.pack_relation_flags(),
            self.ctx.sound_mode(),
        )
    }

    /// Whether a branch proof completed cleanly enough to memoize.
    ///
    /// Degraded proofs are valid for the current stack frame, but caching them
    /// would make a later, cleaner attempt inherit a lazy-resolution miss,
    /// exhausted evaluation fuel, or relation overflow fallback.
    pub(crate) fn generic_constraint_proof_completed_clean(
        &self,
        lazy_failures_at_entry: u64,
    ) -> bool {
        crate::query_boundaries::common::lazy_resolve_failure_count() == lazy_failures_at_entry
            && !self.ctx.types.is_evaluation_fuel_exhausted()
            && !self.ctx.depth_exceeded.get()
            && !self.ctx.relation_overflow.get().has_overflow()
    }

    /// Probe the program-wide
    /// [`crate::context::SharedConstraintProofCache`], if installed.
    ///
    /// Probing needs no shareability gate: only pairs that passed the
    /// publish-side gate can be in a set, so a lookup on an unshareable key
    /// simply misses. This keeps the deep shareability walks off the
    /// cold-lookup path.
    pub(crate) fn shared_constraint_proof_hit(
        &self,
        probe: impl FnOnce(&crate::context::SharedConstraintProofCache) -> bool,
    ) -> bool {
        self.ctx
            .shared_constraint_proofs
            .as_ref()
            .is_some_and(|shared| probe(shared))
    }

    /// Publish-side gate for the program-wide
    /// [`crate::context::SharedConstraintProofCache`]: runs `publish` only
    /// when the just-computed success over `(source, target)` is safe to
    /// share. The proof must not have observed an unresolved `Lazy` def
    /// (`lazy_failures_at_entry` snapshot taken before computing), must not
    /// have run with exhausted evaluation fuel, and must be file-independent
    /// (`constraint_proof_is_program_shareable`). The cheap existence check
    /// comes first so disabled runs skip the deep shareability walks.
    pub(crate) fn publish_shared_constraint_proof(
        &self,
        lazy_failures_at_entry: u64,
        source: TypeId,
        target: TypeId,
        publish: impl FnOnce(&crate::context::SharedConstraintProofCache),
    ) {
        let Some(shared) = &self.ctx.shared_constraint_proofs else {
            return;
        };
        if crate::query_boundaries::common::lazy_resolve_failure_count() == lazy_failures_at_entry
            && !self.ctx.types.is_evaluation_fuel_exhausted()
            && !self.ctx.depth_exceeded.get()
            && !self.ctx.relation_overflow.get().has_overflow()
            && self.constraint_proof_is_program_shareable(source, target)
        {
            publish(shared);
        }
    }

    /// Execute a diagnostic-bearing generic type-argument constraint relation
    /// for raw checker types, preserving the canonical TS2344 request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn type_arg_constraint_relation_outcome(
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
            .type_arg_constraint_relation_successes
            .contains(&cache_key)
        {
            return RELATED_SUCCESS;
        }

        // Program-wide success tier: another file checker may already have
        // proven this exact pair.
        if self.shared_constraint_proof_hit(|s| s.type_arg_relation_successes.contains(&cache_key))
        {
            tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "type_arg", "hit");
            self.ctx
                .type_reference_validation_caches
                .type_arg_constraint_relation_successes
                .insert(cache_key);
            return RELATED_SUCCESS;
        }

        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        let request = crate::query_boundaries::assignability::RelationRequest::type_arg_constraint(
            source, target,
        )
        .with_decision_only();
        let outcome = self.execute_relation_request(&request);
        if outcome.related && !outcome.depth_exceeded && !outcome.iteration_exceeded {
            self.ctx
                .type_reference_validation_caches
                .type_arg_constraint_relation_successes
                .insert(cache_key);
            self.publish_shared_constraint_proof(lazy_failures_at_entry, source, target, |shared| {
                tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "type_arg", "publish");
                shared.type_arg_relation_successes.insert(cache_key);
            });
        }
        outcome
    }

    /// Execute an explicit alias constraint relation for raw checker types,
    /// preserving the canonical explicit-alias request shape. Decision-only:
    /// the sole consumer reads `outcome.related`, so failure analysis is
    /// skipped.
    ///
    /// Successful outcomes are cached by prepared source/target (mirroring
    /// `type_arg_constraint_relation_outcome`): this relation sits directly in
    /// the explicit-alias constraint-validation recursion (#15729), where a
    /// generic alias descent revalidates the same syntactic type reference at
    /// every nesting level as `type_parameter_scope` grows, so an uncached
    /// relation here re-proves the same `(source, target)` pair repeatedly.
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

    /// Execute a diagnostic-bearing conditional true-branch constraint relation
    /// for raw checker types, preserving the canonical true-branch request
    /// shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn conditional_true_branch_constraint_relation_outcome(
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
            .conditional_true_branch_relation_successes
            .contains(&cache_key)
        {
            return RELATED_SUCCESS;
        }

        if self.shared_constraint_proof_hit(|s| {
            s.conditional_true_branch_relation_successes
                .contains(&cache_key)
        }) {
            tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "conditional_true_branch", "hit");
            self.ctx
                .type_reference_validation_caches
                .conditional_true_branch_relation_successes
                .insert(cache_key);
            return RELATED_SUCCESS;
        }

        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        let request =
            crate::query_boundaries::assignability::RelationRequest::conditional_true_branch_constraint(
                source, target,
            )
            .with_decision_only();
        let outcome = self.execute_relation_request(&request);
        if outcome.related && !outcome.depth_exceeded && !outcome.iteration_exceeded {
            self.ctx
                .type_reference_validation_caches
                .conditional_true_branch_relation_successes
                .insert(cache_key);
            self.publish_shared_constraint_proof(lazy_failures_at_entry, source, target, |shared| {
                tracing::trace!(target: "tsz::shared_constraint_proofs", kind = "conditional_true_branch", "publish");
                shared
                    .conditional_true_branch_relation_successes
                    .insert(cache_key);
            });
        }
        outcome
    }
}

use crate::context::GenericConstraintProofKey;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Execute a diagnostic-bearing TS2322 reason-entrypoint relation for raw
    /// checker types, preserving the canonical reason-reporting request shape.
    pub(crate) fn assignability_reason_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        if self.pre_evaluation_index_access_relation_rejects(source, target) {
            let failure = self
                .raw_input_failure_reason(source, target)
                .map(crate::query_boundaries::relation_types::RelationFailure::from_solver_reason);
            return crate::query_boundaries::assignability::RelationOutcome {
                related: false,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure,
                weak_union_violation: false,
                property_classification: None,
            };
        }
        if self.empty_object_deferred_keyof_index_access_accepts(source, target) {
            return crate::query_boundaries::assignability::RelationOutcome {
                related: true,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            };
        }
        if self.same_type_alias_application_uses_conditional_infer(source, target)
            && self.diagnostic_relation_boolean_guard(source, target)
        {
            return crate::query_boundaries::assignability::RelationOutcome {
                related: true,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            };
        }
        if let Some(outcome) = self.variance_accepted_relation_outcome(source, target) {
            return outcome;
        }
        if crate::query_boundaries::assignability::declared_bare_rest_relation_is_raw_sensitive(
            self.ctx.types,
            &self.ctx,
            source,
            target,
        ) {
            self.ensure_relation_inputs_ready(&[source, target]);
            let raw_source = self.substitute_this_type_if_needed(source);
            let raw_target = self.substitute_this_type_if_needed(target);
            let request =
                crate::query_boundaries::assignability::RelationRequest::assignability_reason(
                    raw_source, raw_target,
                );
            let raw_outcome = self.execute_relation_request(&request);
            if !raw_outcome.related {
                return raw_outcome;
            }
        }
        let raw_source = source;
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::assignability_reason(
            source, target,
        );
        let outcome = self.execute_relation_request(&request);
        if !outcome.related
            && self.deferred_index_access_source_constraint_relation_accepts(raw_source, target)
        {
            return crate::query_boundaries::assignability::RelationOutcome {
                related: true,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            };
        }
        outcome
    }

    /// Constraint-widening fallback for a deferred generic indexed-access source.
    ///
    /// When `O[K]` is indexed by a bare type parameter `K extends keyof O`, tsc
    /// keeps it deferred but a deferred `O[K]` is assignable to anything its
    /// constraint — the value-type union `O[keyof O]` reachable through `K`'s
    /// constraint — is assignable to (`getConstraintOfIndexedAccess`). This is
    /// the source-side analogue of the solver's index-access upper-bound check;
    /// it is needed here because evaluating `O[K]` for the relation may
    /// mapped-substitute the source (e.g. `Funcs[K]` → `(x: ArgMap[K]) => void`)
    /// before the solver can recover the index-access shape, so a deferred
    /// `Funcs[K] <= Funcs[keyof ArgMap]` (correlatedUnions) never reaches that
    /// check. Widening the *source* to its constraint is sound (the constraint
    /// is `O[K]`'s upper bound) and only relaxes the source, never the target —
    /// `123 <= Type[K]` is unaffected because `123` is not an index access.
    fn deferred_index_access_source_constraint_relation_accepts(
        &mut self,
        raw_source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some((object_type, index_type)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, raw_source)
        else {
            return false;
        };
        let Some(index_param) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
        else {
            return false;
        };
        let Some(index_constraint) = index_param.constraint else {
            return false;
        };
        self.ensure_relation_input_ready(object_type);
        let evaluated_object = self.evaluate_type_with_env(object_type);
        let value_union = self
            .ctx
            .types
            .factory()
            .index_access(evaluated_object, index_constraint);
        let value_union = self.evaluate_type_with_env(value_union);
        if value_union == raw_source
            || value_union == TypeId::ERROR
            || crate::query_boundaries::common::is_index_access_type(self.ctx.types, value_union)
        {
            return false;
        }
        self.is_assignable_to(value_union, target)
    }

    /// Execute a diagnostic-bearing type-comparability relation for raw
    /// checker types, preserving the canonical comparability request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn type_comparability_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::type_comparability(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a callable-source to union-arm return relation for raw checker
    /// types, preserving the canonical callable-union return request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn callable_union_return_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::callable_union_return(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a callable-source to union-arm parameter relation for raw checker
    /// types, preserving the canonical callable-union parameter request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn callable_union_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::callable_union_parameter(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a type-predicate-to-parameter relation for raw checker types,
    /// preserving the canonical type-predicate parameter request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn type_predicate_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::type_predicate_parameter(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSX children relation for raw checker types,
    /// preserving the canonical JSX children request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn jsx_children_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsx_children(source, target)
                .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSX props relation for raw checker types,
    /// preserving the canonical JSX props request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn jsx_props_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsx_props(source, target)
                .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSX element-type relation for raw checker
    /// types, preserving the canonical `JSX.ElementType` request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn jsx_element_type_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::jsx_element_type(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `for...in` LHS relation for raw checker
    /// types, preserving the canonical `for...in` LHS request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn for_in_lhs_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::for_in_lhs(source, target)
                .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing destructuring-assignment relation for raw
    /// checker types, preserving the canonical destructuring request shape.
    pub(crate) fn destructuring_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::destructuring(source, target);
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing rest-parameter array relation for raw
    /// checker types, preserving the canonical rest-parameter request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn rest_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::rest_parameter(source, target)
                .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing import-attributes relation for raw
    /// checker types, preserving the canonical import-attributes request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn import_attributes_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::import_attributes(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing computed enum-member relation for raw
    /// checker types, preserving the canonical computed-enum request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn computed_enum_member_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::computed_enum_member(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing numeric-enum assignment relation for raw
    /// checker types, preserving the canonical numeric-enum assignment request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn numeric_enum_assignment_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::numeric_enum_assignment(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing type-parameter default relation for raw
    /// checker types, preserving the canonical default-constraint request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn type_parameter_default_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::type_parameter_default(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing index-signature relation for raw checker
    /// types, preserving the canonical index-signature request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn index_signature_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::index_signature(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing decorator-callee relation for raw checker
    /// types, preserving the canonical decorator-callee request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn decorator_callee_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::decorator_callee(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSDoc type-constraint relation for raw
    /// checker types, preserving the canonical JSDoc constraint request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn jsdoc_type_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsdoc_type_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute an array-like constraint element relation for raw checker
    /// types, preserving the canonical array-like request shape.
    /// Decision-only: the sole consumer reads `outcome.related`, so failure
    /// analysis is skipped.
    pub(crate) fn array_like_constraint_element_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::array_like_constraint_element(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing merged-interface constraint relation for
    /// raw checker types, preserving the canonical merged-interface request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn merged_interface_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::merged_interface_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing recursive heritage property relation for
    /// raw checker types, preserving the canonical recursive-heritage request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn recursive_heritage_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::recursive_heritage_property(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a base-union constraint relation for raw checker types,
    /// preserving the canonical union-constraint request shape. Decision-only:
    /// the sole consumer reads `outcome.related`, so failure analysis is
    /// skipped.
    pub(crate) fn union_constraint_member_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::union_constraint_member(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing syntax-instantiated constraint relation
    /// for raw checker types, preserving the canonical syntax request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn syntax_instantiated_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::syntax_instantiated_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

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

    /// Execute a generic type-argument constraint fallback relation while
    /// preserving the `isTypeAssignableTo`-style no-weak policy used by this
    /// TS2344 path.
    pub(crate) fn type_arg_constraint_no_weak_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        self.no_weak_relation_outcome(source, target)
    }

    /// Probe whether an `unknown` type argument satisfies a reduced type-parameter
    /// constraint, i.e. whether the constraint is a top type. `unknown` relates to
    /// the canonical `any`/`unknown` and to structural spellings of them such as
    /// `{} | null | undefined` (TypeScript's `NonReducibleUnknown`).
    /// Decision-only: the caller reads just `outcome.related`.
    pub(crate) fn unknown_type_arg_top_constraint_relation_outcome(
        &mut self,
        constraint: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        self.diagnostic_relation_outcome(TypeId::UNKNOWN, constraint)
    }

    /// Execute a diagnostic-bearing mapped-key constraint relation for raw
    /// checker types, preserving the canonical mapped-key request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn mapped_key_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::mapped_key_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing indexed-access constraint key relation for
    /// raw checker types, preserving the canonical key-space request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn indexed_access_constraint_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::indexed_access_constraint_key(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing indexed-access key-space relation for raw
    /// checker types, preserving the canonical indexed-access request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn indexed_access_key_space_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::indexed_access_key_space(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute the diagnostic-bearing non-keyof index-constraint probe used by
    /// foreign-keyof `TS2536` detection (`constraint -> keyof B`).
    /// Decision-only: the caller reads only `outcome.related`. This wraps the
    /// legacy boolean relation decision directly (no input re-preparation) so
    /// `TS2536` emission/suppression behavior is byte-for-byte preserved while
    /// the probe stays grep-distinct as a named indexed-access outcome.
    pub(crate) fn indexed_access_foreign_keyof_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        self.diagnostic_relation_outcome(source, target)
    }

    /// Execute a diagnostic-bearing conditional constraint component relation
    /// for raw checker types, preserving the canonical conditional request
    /// shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn conditional_constraint_component_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::conditional_constraint_component(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing conditional true-base constraint relation
    /// for raw checker types, preserving the canonical true-base request
    /// shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn conditional_true_base_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::conditional_true_base_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
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

    /// Execute a diagnostic-bearing required mapped constraint relation for
    /// raw checker types, preserving the canonical required-mapped request
    /// shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn required_mapped_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::required_mapped_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing infer-result constraint relation for raw
    /// checker types, preserving the canonical infer-result request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn infer_result_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::infer_result_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute an infer-result constraint fallback relation while preserving
    /// the `isTypeAssignableTo`-style no-weak policy used by this TS2344 path.
    pub(crate) fn infer_result_constraint_no_weak_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        self.no_weak_relation_outcome(source, target)
    }

    /// Execute a diagnostic-bearing generic constraint property relation for
    /// raw checker types, preserving the canonical generic-constraint request
    /// shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn generic_constraint_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::generic_constraint_property(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing property-index-key relation for raw
    /// checker types, preserving the canonical property-index-key request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn property_index_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::property_index_key(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing nullish-error-target relation for raw
    /// checker types, preserving the canonical nullish-target request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn nullish_error_target_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::nullish_error_target(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing duplicate-identifier relation for raw
    /// checker types, preserving the canonical duplicate-identifier request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn duplicate_identifier_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::duplicate_identifier(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing variable-initializer relation for raw
    /// checker types, preserving the canonical initializer request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn variable_initializer_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::variable_initializer(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing contextual binding-default identifier
    /// relation for raw checker types, preserving the canonical identifier request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn identifier_binding_default_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::identifier_binding_default(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `keyof` suppression relation for raw
    /// checker types, preserving the canonical `keyof` suppression request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn keyof_diagnostic_suppression_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::keyof_diagnostic_suppression(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing display-narrowing relation for raw
    /// checker types, preserving the canonical diagnostic-source request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn diagnostic_source_narrowing_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::diagnostic_source_narrowing(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing overlap/comparability relation for raw
    /// checker types, preserving the canonical diagnostic-overlap request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn diagnostic_overlap_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::diagnostic_overlap(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Whether a source satisfies a bare type parameter's constraint, shaped as a
    /// `RelationOutcome` for the "could be instantiated with an arbitrary type
    /// which could be unrelated to" related-information elaboration.
    ///
    /// Routes through `diagnostic_relation_outcome` (the compat-aware
    /// `is_assignable_to` path), not the strict `execute_relation_request` path,
    /// so the related-info is suppressed in exactly the cases where the source
    /// would satisfy the constraint — preserving parity while keeping the call
    /// site off a raw boolean guard.
    pub(crate) fn type_parameter_constraint_elaboration_relation_outcome(
        &mut self,
        source: TypeId,
        constraint: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        self.diagnostic_relation_outcome(source, constraint)
    }

    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn broad_mapped_index_signature_display_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::broad_mapped_index_signature_display(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    pub(crate) fn mapped_object_literal_excess_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::mapped_object_literal_excess_value(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing polymorphic `this` receiver relation for raw
    /// checker types, preserving the canonical receiver request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn polymorphic_this_receiver_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::polymorphic_this_receiver(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class-extends index relation for raw
    /// checker types, preserving the canonical class extends request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn class_extends_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::class_extends_index_value(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class-implements index relation for raw
    /// checker types, preserving the canonical class index request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn class_implements_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::class_implements_index_value(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class-implements whole-type relation for
    /// raw checker types, preserving the canonical class implements request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn class_implements_whole_type_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::class_implements_whole_type(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class static-side relation for raw checker
    /// types, preserving the canonical class static-side request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn class_static_side_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::class_static_side(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing interface-heritage index relation for raw
    /// checker types, preserving the canonical heritage index request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn interface_heritage_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::interface_heritage_index_value(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing interface-heritage generic-method relation
    /// for raw checker types, preserving the canonical heritage method request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn interface_heritage_generic_method_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::interface_heritage_generic_method(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing interface-heritage property/index relation
    /// for raw checker types, preserving the canonical heritage index request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn interface_heritage_property_index_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::interface_heritage_property_index(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSDoc heritage constraint relation for raw
    /// checker types, preserving the canonical JSDoc heritage request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn jsdoc_heritage_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsdoc_heritage_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing missing-property read relation for raw
    /// checker types, preserving the canonical missing-property request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn missing_property_read_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::missing_property_read(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing missing-property write relation for raw
    /// checker types, preserving the canonical missing-property request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn missing_property_write_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::missing_property_write(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing concrete remapped mapped missing-property
    /// relation for raw checker types, preserving the canonical
    /// remapped-mapped request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn concrete_remapped_mapped_missing_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::concrete_remapped_mapped_missing_property(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing exact-optional source filtering relation
    /// for raw checker types, preserving the canonical exact-optional request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn exact_optional_source_filter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::exact_optional_source_filter(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing union excess-property fallback relation for
    /// raw checker types, preserving the canonical union excess request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn union_excess_required_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::union_excess_required_property(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing array-literal contextual-collapse relation
    /// for raw checker types, preserving the contextual-collapse request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn array_literal_contextual_collapse_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::array_literal_contextual_collapse(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute the structural-subtype fallback for array-literal contextual
    /// collapse through a named outcome helper instead of a raw type-computation
    /// relation call.
    pub(crate) fn array_literal_contextual_collapse_subtype_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        self.diagnostic_subtype_outcome(source, target)
    }

    /// Execute a diagnostic-bearing JSX render-fallback relation for raw
    /// checker types, preserving the canonical JSX fallback request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn jsx_render_fallback_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::jsx_render_fallback(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing object-literal mapped contextual key
    /// relation for raw checker types, preserving the canonical mapped-key
    /// request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn object_literal_mapped_contextual_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::object_literal_mapped_contextual_key(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing object-literal computed-key relation for
    /// raw checker types, preserving the canonical computed-key request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn object_literal_computed_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::object_literal_computed_key(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing object-literal JSDoc declared-property
    /// relation for raw checker types, preserving the canonical declared
    /// property request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn object_literal_jsdoc_declared_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::object_literal_jsdoc_declared_property(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing contextual symbol-index value relation for
    /// raw checker types, preserving the canonical symbol-index request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn contextual_symbol_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::contextual_symbol_index_value(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `in`-operator key relation for raw
    /// checker types, preserving the canonical key request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn in_operator_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::in_operator_key(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `in`-operator primitive-constraint
    /// relation for raw checker types, preserving the canonical TS2638 request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn in_operator_primitive_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::in_operator_primitive_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing compound-assignment relation for raw
    /// checker types, preserving the canonical assignment-operation request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn compound_assignment_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::compound_assignment(
            source, target,
        )
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing generic element-write relation for raw
    /// checker types, preserving the canonical deferred write-target request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn generic_element_write_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::generic_element_write(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing property receiver element-display relation
    /// for raw checker types, preserving the canonical receiver-display request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn property_receiver_element_display_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::property_receiver_element_display(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing property receiver index-value relation for
    /// raw checker types, preserving the canonical receiver-display request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn property_receiver_index_value_display_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::property_receiver_index_value_display(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing element-access numeric-index relation for
    /// raw checker types, preserving the canonical TS7015 request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn element_access_number_index_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::element_access_number_index(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing element-access method-suggestion relation
    /// for raw checker types, preserving the canonical suggestion request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn element_access_method_suggestion_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::element_access_method_suggestion(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-elaboration mutual relation for raw
    /// checker types, preserving the canonical call-elaboration request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn call_elaboration_mutual_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_elaboration_mutual(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-display overlap relation for raw
    /// checker types, preserving the canonical display-overlap request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn call_display_overlap_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_display_overlap(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-checker generator-yield relation for
    /// raw checker types, preserving the canonical generator-yield request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn call_generator_yield_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_generator_yield(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `IteratorResult` value relation for raw
    /// checker types, preserving the canonical iterator-result-value request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn iterator_result_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::iterator_result_value(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing round-2 contextual substitution relation
    /// for raw checker types, preserving the canonical substitution request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn round2_contextual_substitution_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::round2_contextual_substitution(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing constructor-inference constraint relation
    /// for raw checker types, preserving the canonical constructor-inference
    /// request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn constructor_inference_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::constructor_inference_constraint(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-adapter compatibility relation for
    /// raw checker types, preserving the canonical call-adapter request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn call_adapter_compatibility_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_adapter_compatibility(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Overload-resolution subtype-pass variant of
    /// [`Self::call_adapter_compatibility_relation_outcome`]: identical
    /// relation shape, but an `any` source is not related to concrete targets
    /// at every nesting level (tsc `chooseOverload` with `subtypeRelation`).
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn overload_subtype_pass_compatibility_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_adapter_compatibility(
                source, target,
            )
            .with_overload_subtype_pass()
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Overload-resolution subtype-pass variant of the bivariant-callback
    /// relation probe used by the call adapter.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn overload_subtype_pass_bivariant_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::bivariant_callbacks(
            source, target,
        )
        .with_overload_subtype_pass()
        .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Overload-resolution subtype-pass variant of the strict relation probe
    /// used by the call adapter.
    pub(crate) fn overload_subtype_pass_strict_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        crate::query_boundaries::assignability::RelationOutcome {
            related: self.is_assignable_to_overload_subtype_pass_strict(source, target),
            depth_exceeded: false,
            iteration_exceeded: false,
            failure: None,
            weak_union_violation: false,
            property_classification: None,
        }
    }

    /// Execute a diagnostic-bearing call-adapter identity fallback relation for
    /// raw checker types, preserving the canonical call-adapter identity shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn call_adapter_identity_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_adapter_identity(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing overload implementation parameter relation
    /// for raw checker types, preserving the canonical overload request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn overload_implementation_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::overload_implementation_parameter(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing binary arithmetic number relation for raw
    /// checker types, preserving the canonical arithmetic operand request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn binary_arithmetic_number_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::binary_arithmetic_number(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing private member access relation for raw
    /// checker types, preserving the canonical private member request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn private_member_access_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::private_member_access(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing function-type relation for raw checker
    /// types, preserving the canonical function-type request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn function_type_compatibility_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::function_type_compatibility(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a namespace-module property mismatch relation for raw checker
    /// types, preserving the canonical downgrade request shape.
    /// Decision-only: every caller reads only `outcome.related`, so the
    /// boundary skips failure analysis and property classification.
    pub(crate) fn namespace_property_mismatch_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::namespace_property_mismatch(
                source, target,
            )
            .with_decision_only();
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `satisfies` relation for raw checker
    /// types, preserving the canonical satisfies relation request shape.
    pub(crate) fn satisfies_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::satisfies(source, target);
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing return-statement relation for raw checker
    /// types, preserving the canonical return relation request shape.
    pub(crate) fn return_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::return_stmt(source, target);
        self.execute_relation_request(&request)
    }
}

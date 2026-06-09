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
            return crate::query_boundaries::assignability::RelationOutcome {
                related: false,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
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
        if let Some(outcome) = self.variance_accepted_relation_outcome(source, target) {
            return outcome;
        }
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::assignability_reason(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing type-comparability relation for raw
    /// checker types, preserving the canonical comparability request shape.
    pub(crate) fn type_comparability_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::type_comparability(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a callable-source to union-arm return relation for raw checker
    /// types, preserving the canonical callable-union return request shape.
    pub(crate) fn callable_union_return_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::callable_union_return(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a callable-source to union-arm parameter relation for raw checker
    /// types, preserving the canonical callable-union parameter request shape.
    pub(crate) fn callable_union_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::callable_union_parameter(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a type-predicate-to-parameter relation for raw checker types,
    /// preserving the canonical type-predicate parameter request shape.
    pub(crate) fn type_predicate_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::type_predicate_parameter(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSX children relation for raw checker types,
    /// preserving the canonical JSX children request shape.
    pub(crate) fn jsx_children_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsx_children(source, target);
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSX props relation for raw checker types,
    /// preserving the canonical JSX props request shape.
    pub(crate) fn jsx_props_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsx_props(source, target);
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSX element-type relation for raw checker
    /// types, preserving the canonical `JSX.ElementType` request shape.
    pub(crate) fn jsx_element_type_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::jsx_element_type(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `for...in` LHS relation for raw checker
    /// types, preserving the canonical `for...in` LHS request shape.
    pub(crate) fn for_in_lhs_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::for_in_lhs(source, target);
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
    pub(crate) fn rest_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::rest_parameter(source, target);
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing import-attributes relation for raw
    /// checker types, preserving the canonical import-attributes request shape.
    pub(crate) fn import_attributes_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::import_attributes(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing computed enum-member relation for raw
    /// checker types, preserving the canonical computed-enum request shape.
    pub(crate) fn computed_enum_member_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::computed_enum_member(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing numeric-enum assignment relation for raw
    /// checker types, preserving the canonical numeric-enum assignment request shape.
    pub(crate) fn numeric_enum_assignment_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::numeric_enum_assignment(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing type-parameter default relation for raw
    /// checker types, preserving the canonical default-constraint request shape.
    pub(crate) fn type_parameter_default_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::type_parameter_default(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing index-signature relation for raw checker
    /// types, preserving the canonical index-signature request shape.
    pub(crate) fn index_signature_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::index_signature(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing decorator-callee relation for raw checker
    /// types, preserving the canonical decorator-callee request shape.
    pub(crate) fn decorator_callee_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::decorator_callee(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSDoc type-constraint relation for raw
    /// checker types, preserving the canonical JSDoc constraint request shape.
    pub(crate) fn jsdoc_type_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsdoc_type_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing explicit alias constraint relation for raw
    /// checker types, preserving the canonical explicit-alias request shape.
    pub(crate) fn explicit_alias_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::explicit_alias_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing array-like constraint element relation for
    /// raw checker types, preserving the canonical array-like request shape.
    pub(crate) fn array_like_constraint_element_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::array_like_constraint_element(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing merged-interface constraint relation for
    /// raw checker types, preserving the canonical merged-interface request shape.
    pub(crate) fn merged_interface_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::merged_interface_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing recursive heritage property relation for
    /// raw checker types, preserving the canonical recursive-heritage request shape.
    pub(crate) fn recursive_heritage_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::recursive_heritage_property(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing base-union constraint relation for raw
    /// checker types, preserving the canonical union-constraint request shape.
    pub(crate) fn union_constraint_member_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::union_constraint_member(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing syntax-instantiated constraint relation
    /// for raw checker types, preserving the canonical syntax request shape.
    pub(crate) fn syntax_instantiated_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::syntax_instantiated_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing generic type-argument constraint relation
    /// for raw checker types, preserving the canonical TS2344 request shape.
    pub(crate) fn type_arg_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
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
            return crate::query_boundaries::assignability::RelationOutcome {
                related: true,
                depth_exceeded: false,
                iteration_exceeded: false,
                failure: None,
                weak_union_violation: false,
                property_classification: None,
            };
        }

        let request = crate::query_boundaries::assignability::RelationRequest::type_arg_constraint(
            source, target,
        );
        let outcome = self.execute_relation_request(&request);
        if outcome.related && !outcome.depth_exceeded && !outcome.iteration_exceeded {
            self.ctx
                .type_reference_validation_caches
                .type_arg_constraint_relation_successes
                .insert(cache_key);
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

    /// Execute a diagnostic-bearing mapped-key constraint relation for raw
    /// checker types, preserving the canonical mapped-key request shape.
    pub(crate) fn mapped_key_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::mapped_key_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing indexed-access constraint key relation for
    /// raw checker types, preserving the canonical key-space request shape.
    pub(crate) fn indexed_access_constraint_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::indexed_access_constraint_key(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing indexed-access key-space relation for raw
    /// checker types, preserving the canonical indexed-access request shape.
    pub(crate) fn indexed_access_key_space_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::indexed_access_key_space(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing conditional constraint component relation
    /// for raw checker types, preserving the canonical conditional request
    /// shape.
    pub(crate) fn conditional_constraint_component_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::conditional_constraint_component(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing conditional true-base constraint relation
    /// for raw checker types, preserving the canonical true-base request
    /// shape.
    pub(crate) fn conditional_true_base_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::conditional_true_base_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing conditional true-branch constraint relation
    /// for raw checker types, preserving the canonical true-branch request
    /// shape.
    pub(crate) fn conditional_true_branch_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::conditional_true_branch_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing required mapped constraint relation for
    /// raw checker types, preserving the canonical required-mapped request
    /// shape.
    pub(crate) fn required_mapped_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::required_mapped_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing infer-result constraint relation for raw
    /// checker types, preserving the canonical infer-result request shape.
    pub(crate) fn infer_result_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::infer_result_constraint(
                source, target,
            );
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
    pub(crate) fn generic_constraint_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::generic_constraint_property(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing property-index-key relation for raw
    /// checker types, preserving the canonical property-index-key request shape.
    pub(crate) fn property_index_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::property_index_key(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing nullish-error-target relation for raw
    /// checker types, preserving the canonical nullish-target request shape.
    pub(crate) fn nullish_error_target_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::nullish_error_target(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing duplicate-identifier relation for raw
    /// checker types, preserving the canonical duplicate-identifier request shape.
    pub(crate) fn duplicate_identifier_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::duplicate_identifier(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing variable-initializer relation for raw
    /// checker types, preserving the canonical initializer request shape.
    pub(crate) fn variable_initializer_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::variable_initializer(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing contextual binding-default identifier
    /// relation for raw checker types, preserving the canonical identifier request shape.
    pub(crate) fn identifier_binding_default_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::identifier_binding_default(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `keyof` suppression relation for raw
    /// checker types, preserving the canonical `keyof` suppression request shape.
    pub(crate) fn keyof_diagnostic_suppression_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::keyof_diagnostic_suppression(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing display-narrowing relation for raw
    /// checker types, preserving the canonical diagnostic-source request shape.
    pub(crate) fn diagnostic_source_narrowing_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::diagnostic_source_narrowing(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing overlap/comparability relation for raw
    /// checker types, preserving the canonical diagnostic-overlap request shape.
    pub(crate) fn diagnostic_overlap_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::diagnostic_overlap(
            source, target,
        );
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

    pub(crate) fn broad_mapped_index_signature_display_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::broad_mapped_index_signature_display(
                source, target,
            );
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
    pub(crate) fn polymorphic_this_receiver_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::polymorphic_this_receiver(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class-extends index relation for raw
    /// checker types, preserving the canonical class extends request shape.
    pub(crate) fn class_extends_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::class_extends_index_value(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class-implements index relation for raw
    /// checker types, preserving the canonical class index request shape.
    pub(crate) fn class_implements_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::class_implements_index_value(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class-implements whole-type relation for
    /// raw checker types, preserving the canonical class implements request shape.
    pub(crate) fn class_implements_whole_type_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::class_implements_whole_type(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing class static-side relation for raw checker
    /// types, preserving the canonical class static-side request shape.
    pub(crate) fn class_static_side_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::class_static_side(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing interface-heritage index relation for raw
    /// checker types, preserving the canonical heritage index request shape.
    pub(crate) fn interface_heritage_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::interface_heritage_index_value(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing interface-heritage generic-method relation
    /// for raw checker types, preserving the canonical heritage method request shape.
    pub(crate) fn interface_heritage_generic_method_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::interface_heritage_generic_method(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing interface-heritage property/index relation
    /// for raw checker types, preserving the canonical heritage index request shape.
    pub(crate) fn interface_heritage_property_index_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::interface_heritage_property_index(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing JSDoc heritage constraint relation for raw
    /// checker types, preserving the canonical JSDoc heritage request shape.
    pub(crate) fn jsdoc_heritage_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::jsdoc_heritage_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing missing-property read relation for raw
    /// checker types, preserving the canonical missing-property request shape.
    pub(crate) fn missing_property_read_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::missing_property_read(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing missing-property write relation for raw
    /// checker types, preserving the canonical missing-property request shape.
    pub(crate) fn missing_property_write_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::missing_property_write(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing concrete remapped mapped missing-property
    /// relation for raw checker types, preserving the canonical
    /// remapped-mapped request shape.
    pub(crate) fn concrete_remapped_mapped_missing_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::concrete_remapped_mapped_missing_property(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing exact-optional source filtering relation
    /// for raw checker types, preserving the canonical exact-optional request shape.
    pub(crate) fn exact_optional_source_filter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::exact_optional_source_filter(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing union excess-property fallback relation for
    /// raw checker types, preserving the canonical union excess request shape.
    pub(crate) fn union_excess_required_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::union_excess_required_property(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing array-literal contextual-collapse relation
    /// for raw checker types, preserving the contextual-collapse request shape.
    pub(crate) fn array_literal_contextual_collapse_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::array_literal_contextual_collapse(
                source, target,
            );
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
    pub(crate) fn jsx_render_fallback_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::jsx_render_fallback(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing object-literal mapped contextual key
    /// relation for raw checker types, preserving the canonical mapped-key
    /// request shape.
    pub(crate) fn object_literal_mapped_contextual_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::object_literal_mapped_contextual_key(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing object-literal computed-key relation for
    /// raw checker types, preserving the canonical computed-key request shape.
    pub(crate) fn object_literal_computed_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::object_literal_computed_key(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing object-literal JSDoc declared-property
    /// relation for raw checker types, preserving the canonical declared
    /// property request shape.
    pub(crate) fn object_literal_jsdoc_declared_property_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::object_literal_jsdoc_declared_property(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing contextual symbol-index value relation for
    /// raw checker types, preserving the canonical symbol-index request shape.
    pub(crate) fn contextual_symbol_index_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::contextual_symbol_index_value(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `in`-operator key relation for raw
    /// checker types, preserving the canonical key request shape.
    pub(crate) fn in_operator_key_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::in_operator_key(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `in`-operator primitive-constraint
    /// relation for raw checker types, preserving the canonical TS2638 request shape.
    pub(crate) fn in_operator_primitive_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::in_operator_primitive_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing compound-assignment relation for raw
    /// checker types, preserving the canonical assignment-operation request shape.
    pub(crate) fn compound_assignment_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::compound_assignment(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing generic element-write relation for raw
    /// checker types, preserving the canonical deferred write-target request shape.
    pub(crate) fn generic_element_write_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::generic_element_write(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing property receiver element-display relation
    /// for raw checker types, preserving the canonical receiver-display request shape.
    pub(crate) fn property_receiver_element_display_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::property_receiver_element_display(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing property receiver index-value relation for
    /// raw checker types, preserving the canonical receiver-display request shape.
    pub(crate) fn property_receiver_index_value_display_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::property_receiver_index_value_display(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing element-access numeric-index relation for
    /// raw checker types, preserving the canonical TS7015 request shape.
    pub(crate) fn element_access_number_index_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::element_access_number_index(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing element-access method-suggestion relation
    /// for raw checker types, preserving the canonical suggestion request shape.
    pub(crate) fn element_access_method_suggestion_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::element_access_method_suggestion(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-elaboration mutual relation for raw
    /// checker types, preserving the canonical call-elaboration request shape.
    pub(crate) fn call_elaboration_mutual_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_elaboration_mutual(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-display overlap relation for raw
    /// checker types, preserving the canonical display-overlap request shape.
    pub(crate) fn call_display_overlap_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::call_display_overlap(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-checker generator-yield relation for
    /// raw checker types, preserving the canonical generator-yield request shape.
    pub(crate) fn call_generator_yield_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request = crate::query_boundaries::assignability::RelationRequest::call_generator_yield(
            source, target,
        );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing `IteratorResult` value relation for raw
    /// checker types, preserving the canonical iterator-result-value request shape.
    pub(crate) fn iterator_result_value_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::iterator_result_value(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing round-2 contextual substitution relation
    /// for raw checker types, preserving the canonical substitution request shape.
    pub(crate) fn round2_contextual_substitution_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::round2_contextual_substitution(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing constructor-inference constraint relation
    /// for raw checker types, preserving the canonical constructor-inference
    /// request shape.
    pub(crate) fn constructor_inference_constraint_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::constructor_inference_constraint(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-adapter compatibility relation for
    /// raw checker types, preserving the canonical call-adapter request shape.
    pub(crate) fn call_adapter_compatibility_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_adapter_compatibility(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing call-adapter identity fallback relation for
    /// raw checker types, preserving the canonical call-adapter identity shape.
    pub(crate) fn call_adapter_identity_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::call_adapter_identity(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing overload implementation parameter relation
    /// for raw checker types, preserving the canonical overload request shape.
    pub(crate) fn overload_implementation_parameter_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::overload_implementation_parameter(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing binary arithmetic number relation for raw
    /// checker types, preserving the canonical arithmetic operand request shape.
    pub(crate) fn binary_arithmetic_number_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::binary_arithmetic_number(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing private member access relation for raw
    /// checker types, preserving the canonical private member request shape.
    pub(crate) fn private_member_access_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::private_member_access(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a diagnostic-bearing function-type relation for raw checker
    /// types, preserving the canonical function-type request shape.
    pub(crate) fn function_type_compatibility_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::function_type_compatibility(
                source, target,
            );
        self.execute_relation_request(&request)
    }

    /// Execute a namespace-module property mismatch relation for raw checker
    /// types, preserving the canonical downgrade request shape.
    pub(crate) fn namespace_property_mismatch_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
    ) -> crate::query_boundaries::assignability::RelationOutcome {
        let (source, target) = self.prepare_assignability_inputs(source, target);
        let request =
            crate::query_boundaries::assignability::RelationRequest::namespace_property_mismatch(
                source, target,
            );
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

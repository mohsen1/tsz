use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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

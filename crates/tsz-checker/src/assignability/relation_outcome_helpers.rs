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

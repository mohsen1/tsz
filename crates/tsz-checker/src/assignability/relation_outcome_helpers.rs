use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
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

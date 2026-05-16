use crate::checkers_domain::call_checker::CallRelationEvidence;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn relation_evidence_for_pair(
        relation_evidence: &[CallRelationEvidence],
        source: TypeId,
        target: TypeId,
    ) -> Option<&crate::query_boundaries::assignability::RelationOutcome> {
        relation_evidence
            .iter()
            .rev()
            .find(|evidence| evidence.source == source && evidence.target == target)
            .map(|evidence| &evidence.outcome)
    }

    pub(crate) fn report_argument_assignability_with_evidence(
        &mut self,
        relation_evidence: &[CallRelationEvidence],
        source: TypeId,
        target: TypeId,
        arg_idx: NodeIndex,
    ) -> bool {
        if let Some(outcome) = Self::relation_evidence_for_pair(relation_evidence, source, target) {
            self.report_argument_assignability_with_outcome(
                source,
                target,
                arg_idx,
                outcome.clone(),
            )
        } else {
            self.check_argument_assignable_or_report(source, target, arg_idx)
        }
    }
}

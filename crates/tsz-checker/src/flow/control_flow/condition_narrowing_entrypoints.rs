use super::FlowAnalyzer;
use super::flow_dp::FlowConditionDpMemos;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::FlowNodeId;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    pub(crate) fn narrow_by_switch_true_case_clause(
        &self,
        type_id: TypeId,
        case_block: NodeIndex,
        clause_idx: NodeIndex,
        case_expr: NodeIndex,
        target: NodeIndex,
    ) -> TypeId {
        let Some(case_block_node) = self.arena.get(case_block) else {
            return self.narrow_type_by_condition(
                type_id,
                case_expr,
                target,
                true,
                FlowNodeId::NONE,
            );
        };
        let Some(case_block_data) = self.arena.get_block(case_block_node) else {
            return self.narrow_type_by_condition(
                type_id,
                case_expr,
                target,
                true,
                FlowNodeId::NONE,
            );
        };

        // For switch(true), direct dispatch into case N requires:
        // - every preceding case condition is false
        // - current case condition is true
        // Fallthrough paths are unioned separately by the switch-clause handler.
        let mut narrowed = type_id;
        let mut saw_current = false;

        for &idx in &case_block_data.statements.nodes {
            let Some(clause_node) = self.arena.get(idx) else {
                continue;
            };
            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                continue;
            };

            if idx == clause_idx {
                saw_current = true;
                if clause.expression.is_some() {
                    narrowed = self.narrow_type_by_condition(
                        narrowed,
                        case_expr,
                        target,
                        true,
                        FlowNodeId::NONE,
                    );
                }
                break;
            }

            if clause.expression.is_some() {
                narrowed = self.narrow_type_by_condition(
                    narrowed,
                    clause.expression,
                    target,
                    false,
                    FlowNodeId::NONE,
                );
            }
        }

        if saw_current {
            narrowed
        } else {
            self.narrow_type_by_condition(type_id, case_expr, target, true, FlowNodeId::NONE)
        }
    }

    /// Apply type narrowing based on a condition expression.
    pub(crate) fn narrow_type_by_condition(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
    ) -> TypeId {
        let mut visited_aliases = AliasCycleTracker::new();

        self.narrow_type_by_condition_inner(
            type_id,
            condition_idx,
            target,
            is_true_branch,
            antecedent_id,
            &mut visited_aliases,
            None,
        )
    }

    pub(crate) fn narrow_type_by_condition_with_dp_memos(
        &self,
        type_id: TypeId,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
        antecedent_id: FlowNodeId,
        dp_memos: &mut FlowConditionDpMemos,
    ) -> TypeId {
        let mut visited_aliases = AliasCycleTracker::new();

        self.narrow_type_by_condition_inner(
            type_id,
            condition_idx,
            target,
            is_true_branch,
            antecedent_id,
            &mut visited_aliases,
            Some(dp_memos),
        )
    }
}

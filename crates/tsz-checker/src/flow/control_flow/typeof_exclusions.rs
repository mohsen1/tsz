use super::FlowAnalyzer;
use super::flow_dp::{DpMemo, DpState};
use tsz_binder::{FlowNodeId, flow_flags};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeofKind;

impl<'a> FlowAnalyzer<'a> {
    pub(crate) const ALL_TYPEOF_EXCLUSIONS: u8 = 0b1111_1111;

    pub(crate) const fn typeof_exclusion_bit(kind: TypeofKind) -> u8 {
        match kind {
            TypeofKind::String => 1 << 0,
            TypeofKind::Number => 1 << 1,
            TypeofKind::Boolean => 1 << 2,
            TypeofKind::BigInt => 1 << 3,
            TypeofKind::Symbol => 1 << 4,
            TypeofKind::Undefined => 1 << 5,
            TypeofKind::Object => 1 << 6,
            TypeofKind::Function => 1 << 7,
        }
    }

    /// Compute the bitmask of typeof-kinds excluded along every reachable path
    /// to `flow_id`. The result is `own_mask | (intersection of antecedent
    /// masks)`, memoized per flow node so the cost is `O(N)` rather than the
    /// previous `O(N · 2^N)` clone-per-branch traversal.
    pub(crate) fn antecedent_typeof_exclusion_mask(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
    ) -> u8 {
        let mut memo: DpMemo<u8> = DpMemo::default();
        self.typeof_exclusion_mask_memoized(flow_id, target, &mut memo)
    }

    fn typeof_exclusion_mask_memoized(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        memo: &mut DpMemo<u8>,
    ) -> u8 {
        if flow_id.is_none() {
            return 0;
        }
        match memo.get(&flow_id) {
            // Back-edge: return the historical fail-safe value (no exclusions)
            // so the surrounding intersection collapses, matching the previous
            // `visited.contains` behavior and avoiding loop-driven narrowing.
            Some(DpState::InProgress) => return 0,
            Some(DpState::Done(value)) => return *value,
            None => {}
        }
        memo.insert(flow_id, DpState::InProgress);

        let value = self.compute_typeof_exclusion_mask(flow_id, target, memo);
        memo.insert(flow_id, DpState::Done(value));
        value
    }

    fn compute_typeof_exclusion_mask(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        memo: &mut DpMemo<u8>,
    ) -> u8 {
        let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
            return 0;
        };
        if flow.has_any_flags(flow_flags::UNREACHABLE) {
            return 0;
        }

        let own = if flow.has_any_flags(flow_flags::CONDITION) {
            self.typeof_exclusion_for_condition(
                flow.node,
                target,
                flow.has_any_flags(flow_flags::TRUE_CONDITION),
            )
            .map_or(0, Self::typeof_exclusion_bit)
        } else {
            0
        };

        if flow.antecedent.is_empty() {
            return own;
        }

        let mut common_antecedent_mask = None;
        // Snapshot antecedents so we can release the borrow on `flow_nodes`
        // before recursing — recursion may walk the same arena.
        let antecedents: Vec<FlowNodeId> = flow.antecedent.to_vec();
        for antecedent in antecedents {
            if antecedent.is_none() {
                continue;
            }
            if self
                .binder
                .flow_nodes
                .get(antecedent)
                .is_some_and(|antecedent_flow| {
                    antecedent_flow.has_any_flags(flow_flags::UNREACHABLE)
                })
            {
                continue;
            }
            let mask = self.typeof_exclusion_mask_memoized(antecedent, target, memo);
            common_antecedent_mask = Some(match common_antecedent_mask {
                Some(common) => common & mask,
                None => mask,
            });
        }

        own | common_antecedent_mask.unwrap_or(0)
    }

    pub(crate) fn flow_has_exhaustive_typeof_exclusions(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
    ) -> bool {
        self.antecedent_typeof_exclusion_mask(flow_id, target) == Self::ALL_TYPEOF_EXCLUSIONS
    }

    pub(crate) fn typeof_exclusion_for_condition(
        &self,
        condition_idx: NodeIndex,
        target: NodeIndex,
        is_true_branch: bool,
    ) -> Option<TypeofKind> {
        let condition_idx = self.skip_parenthesized(condition_idx);
        let cond_node = self.arena.get(condition_idx)?;

        if cond_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            && let Some(unary) = self.arena.get_unary_expr(cond_node)
            && unary.operator == SyntaxKind::ExclamationToken as u16
        {
            return self.typeof_exclusion_for_condition(unary.operand, target, !is_true_branch);
        }

        let bin = self.arena.get_binary_expr(cond_node)?;
        let kind = TypeofKind::parse(self.typeof_comparison_literal(bin.left, bin.right, target)?)?;

        let effective_sense = if bin.operator_token
            == SyntaxKind::ExclamationEqualsEqualsToken as u16
            || bin.operator_token == SyntaxKind::ExclamationEqualsToken as u16
        {
            !is_true_branch
        } else {
            is_true_branch
        };
        (!effective_sense).then_some(kind)
    }
}

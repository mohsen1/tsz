use super::FlowAnalyzer;
use super::flow_dp::{DpMemo, resolve_backward_dp};
use crate::query_boundaries::common::TypeofKind;
use tsz_binder::{FlowNodeId, flow_flags};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

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
    /// to `flow_id`: `own_mask | (intersection of antecedent masks)`, folded
    /// iteratively over the flow graph (see [`resolve_backward_dp`]). Each node
    /// is computed once, so the cost is `O(N)` and the native-stack depth is
    /// bounded regardless of how long the antecedent chain is.
    pub(crate) fn antecedent_typeof_exclusion_mask(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
    ) -> u8 {
        let mut memo: DpMemo<u8> = DpMemo::default();
        self.antecedent_typeof_exclusion_mask_with_memo(flow_id, target, &mut memo)
    }

    pub(crate) fn antecedent_typeof_exclusion_mask_with_memo(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        memo: &mut DpMemo<u8>,
    ) -> u8 {
        // Back-edge / no-information element is `0` (no exclusions), so the
        // surrounding intersection collapses and loops drive no narrowing,
        // matching the previous recursive `DpState::InProgress` arm.
        resolve_backward_dp(
            flow_id,
            memo,
            0,
            |node| self.typeof_exclusion_antecedents(node),
            |node, antecedent_masks| {
                self.typeof_exclusion_mask_fold(node, target, antecedent_masks)
            },
        )
    }

    /// The antecedents the mask traversal descends into: non-`none`,
    /// non-unreachable predecessors of a reachable node. An unreachable (or
    /// missing) node contributes nothing and is not traversed past.
    fn typeof_exclusion_antecedents(
        &self,
        flow_id: FlowNodeId,
    ) -> smallvec::SmallVec<[FlowNodeId; 2]> {
        let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
            return smallvec::SmallVec::new();
        };
        if flow.has_any_flags(flow_flags::UNREACHABLE) {
            return smallvec::SmallVec::new();
        }
        flow.antecedent
            .iter()
            .copied()
            .filter(|antecedent| {
                !antecedent.is_none()
                    && !self
                        .binder
                        .flow_nodes
                        .get(*antecedent)
                        .is_some_and(|antecedent_flow| {
                            antecedent_flow.has_any_flags(flow_flags::UNREACHABLE)
                        })
            })
            .collect()
    }

    /// Combine a node's own typeof-exclusion bit with the intersection of its
    /// antecedents' masks. Matches the previous recursive `compute`: an
    /// unreachable/missing node yields `0`, a node with no reachable
    /// antecedents yields just its own bit, otherwise `own | intersection`.
    fn typeof_exclusion_mask_fold(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        antecedent_masks: &[u8],
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

        let common_antecedent_mask = antecedent_masks
            .iter()
            .copied()
            .reduce(|common, mask| common & mask);

        own | common_antecedent_mask.unwrap_or(0)
    }

    pub(crate) fn flow_has_exhaustive_typeof_exclusions(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
    ) -> bool {
        self.antecedent_typeof_exclusion_mask(flow_id, target) == Self::ALL_TYPEOF_EXCLUSIONS
    }

    pub(crate) fn flow_has_exhaustive_typeof_exclusions_with_memo(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        memo: &mut DpMemo<u8>,
    ) -> bool {
        self.antecedent_typeof_exclusion_mask_with_memo(flow_id, target, memo)
            == Self::ALL_TYPEOF_EXCLUSIONS
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

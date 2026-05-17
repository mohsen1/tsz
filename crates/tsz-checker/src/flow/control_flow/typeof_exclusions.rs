use super::FlowAnalyzer;
use rustc_hash::FxHashMap;
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

    /// Uses memoization so each flow node is evaluated exactly once (O(N) per call).
    /// The sentinel 0 (no exclusions) is inserted before recursion: if a back-edge
    /// returns to this node mid-traversal, the cycle contributes nothing, which is
    /// the correct conservative answer for an "all paths" intersection.
    pub(crate) fn antecedent_typeof_exclusion_mask_memoized(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        memo: &mut FxHashMap<FlowNodeId, u8>,
    ) -> u8 {
        if flow_id.is_none() {
            return 0;
        }
        if let Some(&cached) = memo.get(&flow_id) {
            return cached;
        }

        memo.insert(flow_id, 0);

        let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
            return 0;
        };
        if flow.has_any_flags(flow_flags::UNREACHABLE) {
            memo.insert(flow_id, 0);
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
            memo.insert(flow_id, own);
            return own;
        }

        let mut common_antecedent_mask = None;
        for &ant in flow.antecedent.iter().filter(|&&ant| {
            !ant.is_none()
                && !self
                    .binder
                    .flow_nodes
                    .get(ant)
                    .is_some_and(|f| f.has_any_flags(flow_flags::UNREACHABLE))
        }) {
            let mask = self.antecedent_typeof_exclusion_mask_memoized(ant, target, memo);
            common_antecedent_mask = Some(match common_antecedent_mask {
                Some(common) => common & mask,
                None => mask,
            });
        }

        let result = own | common_antecedent_mask.unwrap_or(0);
        memo.insert(flow_id, result);
        result
    }

    pub(crate) fn flow_has_exhaustive_typeof_exclusions(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
    ) -> bool {
        let mut memo = FxHashMap::default();
        self.antecedent_typeof_exclusion_mask_memoized(flow_id, target, &mut memo)
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

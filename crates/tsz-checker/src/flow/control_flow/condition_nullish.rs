use super::FlowAnalyzer;
use super::flow_dp::{DpMemo, resolve_backward_dp};
use tsz_binder::{FlowNodeId, flow_flags};
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    /// Returns `true` when every reachable antecedent path through `flow_id`
    /// has compared `target` to `null`. The traversal is memoized per flow
    /// node so it runs in `O(N)` and produces the same answer regardless of
    /// the order in which DAG-shared antecedents are visited; the previous
    /// implementation shared a single `visited` `Vec` across siblings, which
    /// made the second branch see shared antecedents as already-visited and
    /// (silently, incorrectly) collapsed the AND to `false`.
    pub(super) fn antecedent_chain_excludes_null_for_target(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
    ) -> bool {
        let mut memo: DpMemo<bool> = DpMemo::default();
        // Back-edge / no-information element is `false` (treat the loop as not
        // contributing a null-exclusion) so loops do not over-narrow, matching
        // the previous recursive `DpState::InProgress` arm. Folded iteratively
        // (see `resolve_backward_dp`) so a long antecedent chain cannot exhaust
        // the native stack.
        resolve_backward_dp(
            flow_id,
            &mut memo,
            false,
            |node| self.null_exclusion_antecedents(node),
            |node, antecedent_values| self.excludes_null_fold(node, target, antecedent_values),
        )
    }

    /// The antecedents the null-exclusion traversal descends into: the
    /// non-`none` predecessors. Unlike the typeof-exclusion mask, this analysis
    /// historically did not filter unreachable antecedents, so that behavior is
    /// preserved here.
    fn null_exclusion_antecedents(
        &self,
        flow_id: FlowNodeId,
    ) -> smallvec::SmallVec<[FlowNodeId; 2]> {
        let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
            return smallvec::SmallVec::new();
        };
        flow.antecedent
            .iter()
            .copied()
            .filter(|antecedent| !antecedent.is_none())
            .collect()
    }

    /// `true` when `flow_id` itself is a `target`-null comparison, or every one
    /// of its antecedents excludes null. Matches the previous recursive
    /// `compute`: a node with no antecedents that is not itself a null
    /// comparison does not exclude null.
    fn excludes_null_fold(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        antecedent_values: &[bool],
    ) -> bool {
        let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
            return false;
        };
        if flow.has_any_flags(flow_flags::CONDITION)
            && self.condition_branch_excludes_null_for_target(flow, target)
        {
            return true;
        }

        !antecedent_values.is_empty() && antecedent_values.iter().all(|&excludes| excludes)
    }

    fn condition_branch_excludes_null_for_target(
        &self,
        flow: &tsz_binder::FlowNode,
        target: NodeIndex,
    ) -> bool {
        let Some(node) = self.arena.get(flow.node) else {
            return false;
        };
        let Some(bin) = self.arena.get_binary_expr(node) else {
            return false;
        };
        let (is_equals, is_strict) = match bin.operator_token {
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => (true, true),
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => (false, true),
            k if k == SyntaxKind::EqualsEqualsToken as u16 => (true, false),
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => (false, false),
            _ => return false,
        };
        let Some(nullish) = self.nullish_comparison(bin.left, bin.right, target) else {
            return false;
        };
        let is_true_branch = flow.has_any_flags(flow_flags::TRUE_CONDITION);
        let effective_truth = if is_equals {
            is_true_branch
        } else {
            !is_true_branch
        };

        if is_strict {
            nullish == TypeId::NULL && !effective_truth
        } else {
            !effective_truth
        }
    }
}

use super::FlowAnalyzer;
use super::flow_dp::{DpMemo, DpState};
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
        self.excludes_null_memoized(flow_id, target, &mut memo)
    }

    fn excludes_null_memoized(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        memo: &mut DpMemo<bool>,
    ) -> bool {
        if flow_id.is_none() {
            return false;
        }
        match memo.get(&flow_id) {
            // Back-edge: preserve the historical fail-safe (treat the loop as
            // not contributing a null-exclusion) so loops do not over-narrow.
            Some(DpState::InProgress) => return false,
            Some(DpState::Done(value)) => return *value,
            None => {}
        }
        memo.insert(flow_id, DpState::InProgress);

        let value = self.compute_excludes_null(flow_id, target, memo);
        memo.insert(flow_id, DpState::Done(value));
        value
    }

    fn compute_excludes_null(
        &self,
        flow_id: FlowNodeId,
        target: NodeIndex,
        memo: &mut DpMemo<bool>,
    ) -> bool {
        let Some(flow) = self.binder.flow_nodes.get(flow_id) else {
            return false;
        };
        if flow.has_any_flags(flow_flags::CONDITION)
            && self.condition_branch_excludes_null_for_target(flow, target)
        {
            return true;
        }

        let mut saw_antecedent = false;
        // Snapshot so we can release the borrow on `flow_nodes` before
        // recursing into siblings. Most flow nodes have one or two antecedents,
        // so keep that snapshot stack-backed in the common case.
        let antecedents: smallvec::SmallVec<[FlowNodeId; 2]> =
            flow.antecedent.iter().copied().collect();
        for antecedent in antecedents {
            if antecedent.is_none() {
                continue;
            }
            saw_antecedent = true;
            if !self.excludes_null_memoized(antecedent, target, memo) {
                return false;
            }
        }
        saw_antecedent
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

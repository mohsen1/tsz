//! Per-walk memoization for the backward flow walk's defer / CALL
//! narrow-divert classification.
//!
//! When `check_flow` crosses a CALL (or `await`/`yield`) flow node it must
//! decide whether that node can narrow or divert the queried reference. Both
//! that decision (`call_node_may_narrow_or_divert`) and the defer decision
//! (`antecedent_requires_defer`) are pure functions of a flow node within a
//! single backward walk: the queried `reference`/`symbol_id` are fixed for the
//! walk and the type / type-predicate caches are immutable mid-walk. The
//! linear-passthrough chase re-scans overlapping pass-through runs on every
//! worklist pop and the defer classifier recurses over pass-through call
//! chains, so without a memo a call-dense scope re-extracts each call's
//! predicate signature thousands of times per reference read. Caching each
//! result by flow-node id collapses that to one classification per node per
//! walk with no change in value.

use super::FlowAnalyzer;
use rustc_hash::FxHashMap;
use tsz_binder::{FlowNode, FlowNodeId, SymbolId};
use tsz_parser::parser::NodeIndex;

/// Per-walk classification memos shared by the linear-passthrough chase and the
/// defer classifier. Every decision is a pure function of a flow node within a
/// single backward walk (`reference`/`symbol_id` are fixed and the type / type-
/// predicate caches are immutable mid-walk), so each is computed at most once per
/// node per walk. Bundling them keeps the chase within its argument budget while
/// the recursion in `antecedent_requires_defer` reuses the tables.
///
/// INVARIANT: an instance is created fresh at the start of a single `check_flow`
/// walk and never reused across walks. That is what makes keying purely by
/// `FlowNodeId` sound — the `reference`/`symbol_id` a result depends on are
/// constant for the instance's lifetime, so they need not enter the key. A future
/// change that hoisted/shared a `FlowDeferMemos` across walks would silently serve
/// stale verdicts for a different reference and must re-key by reference instead.
#[derive(Default)]
pub(crate) struct FlowDeferMemos {
    /// `antecedent_requires_defer` result keyed by flow-node id.
    pub(crate) defer: FxHashMap<FlowNodeId, bool>,
    /// `call_node_may_narrow_or_divert` result keyed by flow-node id.
    pub(crate) call_divert: FxHashMap<FlowNodeId, bool>,
    /// `assignment_relevant_to_reference` result keyed by flow-node id — whether a
    /// pure ASSIGNMENT flow node targets or affects the queried reference, the
    /// gate the linear-passthrough chase uses to decide whether it may splice the
    /// node. Pure per walk (`reference`/`symbol_id` are fixed), recomputed on every
    /// chase re-scan of an overlapping assignment run, so memoized like the two
    /// classifications above.
    pub(crate) assignment_relevant: FxHashMap<FlowNodeId, bool>,
}

impl FlowAnalyzer<'_> {
    /// Per-walk memoized wrapper around [`Self::call_node_may_narrow_or_divert`].
    ///
    /// The classification is a pure function of the CALL flow node within a single
    /// backward walk (it reads the immutable per-check type cache and resolved
    /// type-predicate tables; `reference`/`symbol_id` do not enter it). Both the
    /// linear-passthrough chase and the defer classifier re-derive it for the same
    /// node many times per walk — the chase alone re-scans overlapping pass-through
    /// runs on every worklist pop — so without a memo a call-dense scope pays the
    /// full predicate-signature extraction (`classify_for_predicate_signature`,
    /// `callable_shape`) thousands of times per reference read. Caching by flow-node
    /// id collapses that to one extraction per node per walk with no change in value.
    pub(crate) fn call_node_may_narrow_or_divert_cached(
        &self,
        flow_id: FlowNodeId,
        ant_flow: &FlowNode,
        memo: &mut FxHashMap<FlowNodeId, bool>,
    ) -> bool {
        if let Some(&cached) = memo.get(&flow_id) {
            return cached;
        }
        let result = self.call_node_may_narrow_or_divert(ant_flow);
        memo.insert(flow_id, result);
        result
    }

    /// Whether a pure ASSIGNMENT flow node *targets* or *affects* `reference` — the
    /// relevance gate the linear-passthrough chase uses to decide whether the node
    /// is a pure pass-through it may splice. Mirrors the worklist's own ASSIGNMENT
    /// pre-filter: the #13404 O(1) root-symbol overlap probe rejects unrelated
    /// assignments cheaply, and only on a possible overlap does it fall back to the
    /// deep `assignment_targets_reference_node` / `assignment_affects_reference_node`
    /// AST predicates. The result is a pure function of
    /// `(assignment_node, reference, symbol_id)`.
    fn assignment_relevant_to_reference(
        &self,
        assignment_node: NodeIndex,
        reference: NodeIndex,
        symbol_id: Option<SymbolId>,
    ) -> bool {
        if !self.assignment_root_symbols_may_overlap(assignment_node, reference, symbol_id) {
            false
        } else if let Some(target_sym) = symbol_id {
            let assignment_sym = self.reference_symbol(assignment_node);
            if assignment_sym.is_some() && assignment_sym != Some(target_sym) {
                // Different binder symbol: cannot target the reference. It may still
                // *affect* a property/element path of the reference.
                self.assignment_affects_reference_node(assignment_node, reference)
            } else {
                self.assignment_targets_reference_node(assignment_node, reference)
                    || self.assignment_affects_reference_node(assignment_node, reference)
            }
        } else {
            self.assignment_targets_reference_node(assignment_node, reference)
                || self.assignment_affects_reference_node(assignment_node, reference)
        }
    }

    /// Per-walk memoized wrapper around [`Self::assignment_relevant_to_reference`].
    ///
    /// The relevance decision is a pure function of the ASSIGNMENT flow node within
    /// a single backward walk (`reference`/`symbol_id` are fixed and the binder
    /// symbol / AST shapes are immutable mid-walk). The linear-passthrough chase
    /// re-derives it for the same node on every overlapping re-scan — each interior
    /// node a surviving merge re-schedules re-runs the chase over the run ahead of
    /// it — so without a memo an assignment-dense scope pays the full root-overlap
    /// probe plus targeting/affecting AST comparison many times per reference read.
    /// Caching by flow-node id collapses that to one classification per node per
    /// walk with no change in value.
    pub(crate) fn assignment_relevant_to_reference_cached(
        &self,
        flow_id: FlowNodeId,
        assignment_node: NodeIndex,
        reference: NodeIndex,
        symbol_id: Option<SymbolId>,
        memo: &mut FxHashMap<FlowNodeId, bool>,
    ) -> bool {
        if let Some(&cached) = memo.get(&flow_id) {
            return cached;
        }
        let result = self.assignment_relevant_to_reference(assignment_node, reference, symbol_id);
        memo.insert(flow_id, result);
        result
    }
}

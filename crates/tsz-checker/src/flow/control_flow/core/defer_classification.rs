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
use tsz_binder::{FlowNode, FlowNodeId};

/// Per-walk classification memos shared by the linear-passthrough chase and the
/// defer classifier. Both decisions are pure functions of a flow node within a
/// single backward walk (`reference`/`symbol_id` are fixed and the type / type-
/// predicate caches are immutable mid-walk), so each is computed at most once per
/// node per walk. Bundling them keeps the chase within its argument budget while
/// the recursion in `antecedent_requires_defer` reuses both tables.
#[derive(Default)]
pub(crate) struct FlowDeferMemos {
    /// `antecedent_requires_defer` result keyed by flow-node id.
    pub(crate) defer: FxHashMap<FlowNodeId, bool>,
    /// `call_node_may_narrow_or_divert` result keyed by flow-node id.
    pub(crate) call_divert: FxHashMap<FlowNodeId, bool>,
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
}

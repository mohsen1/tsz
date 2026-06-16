//! Per-switch all-distinct-literals memo for discriminated-union narrowing.
//!
//! Decides once per case block whether every clause is a pairwise-distinct
//! literal label, so the per-clause predecessor scan in
//! [`FlowAnalyzer::narrow_by_switch_case_clause`](super::FlowAnalyzer) collapses
//! from O(N^2) to O(N) over an N-arm switch. See #13598.

use super::FlowAnalyzer;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> FlowAnalyzer<'a> {
    /// Shared per-switch all-distinct-literals memo, when wired.
    #[inline]
    fn shared_switch_all_distinct_literals_cache(
        &self,
    ) -> Option<&'a RefCell<FxHashMap<u32, bool>>> {
        self.shared
            .map(|s| &s.flow_switch_all_distinct_literals_cache)
    }

    /// Whether every clause in `case_block` is a recognized literal label and
    /// all such labels are pairwise distinct (and there is no `default`),
    /// memoized per case block.
    ///
    /// When this holds, the per-clause predecessor scan in
    /// [`Self::narrow_by_switch_case_clause`] — which exists only to confirm
    /// "all earlier cases are distinct literals" before taking the
    /// no-exclusion fast path — is *always* satisfied for every clause, so the
    /// O(N) per-clause scans (O(N^2) over the switch) collapse to a single O(N)
    /// pass computed once here. The property is a pure function of the
    /// immutable post-bind case block (same invariant as
    /// [`Self::cached_case_clause_literal_type`]).
    ///
    /// A `false` result is fully behavior-preserving: the caller falls through
    /// to the existing per-clause logic, which produces identical narrowing.
    pub(crate) fn switch_case_block_all_distinct_literals(&self, case_block: NodeIndex) -> bool {
        if let Some(cache) = self.shared_switch_all_distinct_literals_cache()
            && let Some(&hit) = cache.borrow().get(&case_block.0)
        {
            return hit;
        }
        let computed = self.compute_switch_case_block_all_distinct_literals(case_block);
        if let Some(cache) = self.shared_switch_all_distinct_literals_cache() {
            cache.borrow_mut().insert(case_block.0, computed);
        }
        computed
    }

    fn compute_switch_case_block_all_distinct_literals(&self, case_block: NodeIndex) -> bool {
        let Some(case_block_data) = self
            .arena
            .get(case_block)
            .and_then(|node| self.arena.get_block(node))
        else {
            return false;
        };
        let mut seen: FxHashSet<TypeId> = FxHashSet::with_capacity_and_hasher(
            case_block_data.statements.nodes.len(),
            <_>::default(),
        );
        for &idx in &case_block_data.statements.nodes {
            let Some(clause_node) = self.arena.get(idx) else {
                return false;
            };
            let Some(clause) = self.arena.get_case_clause(clause_node) else {
                return false;
            };
            // A `default` clause has no expression: it is not a literal label,
            // so the "all earlier cases are distinct literals" predecessor scan
            // can fail for clauses after it. Decline the fast path.
            if clause.expression.is_none() {
                return false;
            }
            let Some(lit) = self.cached_case_clause_literal_type(clause.expression) else {
                return false;
            };
            if !seen.insert(lit) {
                // Duplicate literal label: a later clause sharing this literal
                // would see a matching predecessor, so the fast path cannot
                // apply uniformly.
                return false;
            }
        }
        true
    }
}

//! Shared memoized DP helpers for backward flow-graph traversals.
//!
//! Several flow analyses fold information across the reachable antecedents of a
//! flow node ("for all paths leading here, is property P true?"). Naively
//! cloning the visited set per branch is `O(N · 2^N)` on diamond-shaped graphs
//! and blows past the conformance per-test timeout once `N` reaches ~50 (see
//! issue #7682). The right shape is a single memoized traversal: each node's
//! result depends only on its own flags and the memoized results of its
//! antecedents, so each node is computed once.
//!
//! Two sentinels matter:
//! - `NotVisited`: the node has never been entered. Compute it.
//! - `InProgress`: the node is on the current recursion stack — a CFG back-edge
//!   (loop) reached itself. We return the analysis's *no-information* value so
//!   the fold operator treats it as the identity element of the other branches.
//!   For "AND across antecedents" (null-exclusion) that is `false` (forces the
//!   loop to be evaluated by its acyclic predecessors). For "intersection of
//!   typeof-exclusion masks" that is `0` for the same reason. This preserves
//!   tsz's previous, fail-safe (no-narrow-on-loop) behavior while collapsing
//!   the asymptotic cost from exponential to linear.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::collections::VecDeque;
use tsz_binder::FlowNodeId;

#[derive(Clone, Copy)]
pub(crate) enum DpState<T: Copy> {
    InProgress,
    Done(T),
}

/// Memo table keyed by flow node, storing either an in-progress sentinel or
/// the final computed value. Callers materialize one per top-level entry; the
/// table is per-traversal, not shared across queries, so it does not need to
/// participate in the broader checker cache plumbing.
pub(crate) type DpMemo<T> = FxHashMap<FlowNodeId, DpState<T>>;

/// Scratch DP memos for one `check_flow` traversal and one reference target.
///
/// These are intentionally scoped to a single flow query: all analyses key
/// only by `FlowNodeId`, while the typeof/null answers also depend on the
/// target expression being narrowed. Reusing those across references would be
/// unsound; reusing them within one `check_flow` call avoids rebuilding the
/// same graph folds for every worklist visit. The switch-chain memo is a pure
/// property of the flow graph (target-independent) but shares the per-call
/// lifetime for simplicity — the flow graph is immutable for the duration of
/// one `check_flow` traversal, so its verdicts stay valid for the whole call.
#[derive(Default)]
pub(crate) struct FlowConditionDpMemos {
    pub(crate) typeof_exclusions: DpMemo<u8>,
    pub(crate) null_exclusions: DpMemo<bool>,
    pub(crate) switch_chains: ChainReachabilityMemo,
}

impl FlowConditionDpMemos {
    pub(crate) fn clear(&mut self) {
        self.typeof_exclusions.clear();
        self.null_exclusions.clear();
        self.switch_chains.clear();
    }
}

/// Memo plus reusable scratch buffers for exact "does any antecedent path
/// from this node contain a flagged node" reachability queries (currently:
/// `SWITCH_CLAUSE` containment).
///
/// Unlike the fold-based DP below, flag reachability through CFG back-edges
/// has no no-information identity element: resolving an in-progress loop
/// header as "no flag" memoizes a wrong `false` for loop-body nodes whose
/// only route to the flagged node runs through that header (a `switch`
/// statement feeding a `while` loop is enough to hit this shape). The BFS in
/// [`resolve_chain_reachability`] therefore memoizes only proven verdicts:
///
/// - when a query exhausts its frontier without a hit, every node it visited
///   had its entire reachable set explored (within `visited` plus
///   already-proven-`false` nodes), so all of them are `false`;
/// - when a query reaches a flagged (or already-proven-`true`) node, every
///   node on the discovery path back to the query root reaches it, so the
///   whole path is `true`.
///
/// Both common cases collapse to an O(1) verdict lookup on later queries of
/// the same `check_flow` worklist, and the scratch buffers are reused across
/// queries so no per-worklist-iteration allocation remains (issue #13083).
#[derive(Default)]
pub(crate) struct ChainReachabilityMemo {
    verdicts: FxHashMap<FlowNodeId, bool>,
    queue: VecDeque<FlowNodeId>,
    visited: FxHashSet<FlowNodeId>,
    /// Maps each newly discovered node to the (downstream) node whose
    /// antecedent list it was discovered through.
    discovered_from: FxHashMap<FlowNodeId, FlowNodeId>,
}

impl ChainReachabilityMemo {
    pub(crate) fn clear(&mut self) {
        self.verdicts.clear();
        self.queue.clear();
        self.visited.clear();
        self.discovered_from.clear();
    }
}

/// Exact memoized backward reachability of a flagged flow node.
///
/// Returns `true` when `root` is flagged or any node in its transitive
/// antecedent chain is. Verdicts are memoized per node in `memo` and are pure
/// properties of the (immutable-during-traversal) flow graph, so they are
/// independent of query order — including for nodes on CFG cycles, where the
/// fold-based [`resolve_backward_dp`] would memoize an order-dependent
/// under-approximation.
///
/// `antecedents_of` must return the non-`none` antecedents of a node (nodes
/// without a flow entry return no antecedents); `is_flagged` reports whether
/// the node itself carries the searched flag.
pub(crate) fn resolve_chain_reachability<FAnts, FFlag>(
    root: FlowNodeId,
    memo: &mut ChainReachabilityMemo,
    antecedents_of: FAnts,
    is_flagged: FFlag,
) -> bool
where
    FAnts: Fn(FlowNodeId) -> SmallVec<[FlowNodeId; 2]>,
    FFlag: Fn(FlowNodeId) -> bool,
{
    if root.is_none() {
        return false;
    }
    if let Some(&verdict) = memo.verdicts.get(&root) {
        return verdict;
    }

    memo.queue.clear();
    memo.visited.clear();
    memo.discovered_from.clear();
    memo.queue.push_back(root);
    memo.visited.insert(root);

    let mut found: Option<FlowNodeId> = None;
    while let Some(current) = memo.queue.pop_front() {
        match memo.verdicts.get(&current) {
            Some(true) => {
                found = Some(current);
                break;
            }
            // Proven flag-free by an earlier query: its entire reachable set
            // was already explored, so do not re-expand it.
            Some(false) => continue,
            None => {}
        }
        if is_flagged(current) {
            memo.verdicts.insert(current, true);
            found = Some(current);
            break;
        }
        for antecedent in antecedents_of(current) {
            if antecedent.is_none() || !memo.visited.insert(antecedent) {
                continue;
            }
            memo.discovered_from.insert(antecedent, current);
            memo.queue.push_back(antecedent);
        }
    }

    if let Some(found) = found {
        // Every node on the discovery path `root -> .. -> found` has `found`
        // in its antecedent chain, so the whole path shares the verdict. The
        // worklist queries successive upstream nodes, which lie on this path,
        // so marking it keeps repeated queries O(1).
        let mut node = found;
        loop {
            memo.verdicts.insert(node, true);
            match memo.discovered_from.get(&node) {
                Some(&downstream) => node = downstream,
                None => break,
            }
        }
        return true;
    }

    // Exhausted without a hit: every visited node's reachable set lies within
    // `visited` plus already-proven-`false` nodes, none of which are flagged,
    // so all visited nodes are proven `false`.
    for &node in &memo.visited {
        memo.verdicts.insert(node, false);
    }
    false
}

/// Drive a backward flow-graph DP fold *iteratively* over an explicit heap
/// stack rather than the native call stack.
///
/// The recursive shape (`compute(node)` folds `dp(antecedent)` for each
/// antecedent, memoizing per node) is `O(N)` in total work but recurses to a
/// depth equal to the longest acyclic antecedent chain. Real-world fixtures
/// (e.g. the `effect` canary's large modules) produce antecedent chains long
/// enough to exhaust even the 128MB checker stack, aborting the whole compile.
/// Folding over an explicit `Vec` stack keeps the work `O(N)` while bounding
/// native-stack depth to a constant.
///
/// Semantics match the previous recursion exactly:
/// - `none` flow ids resolve to `in_progress_value` (the analysis's
///   no-information element).
/// - A node still `InProgress` when read by a descendant is a CFG back-edge
///   (loop); it resolves to `in_progress_value`, so the fold treats the loop as
///   contributing no information, identical to the recursive
///   `DpState::InProgress` arm.
/// - `antecedents_of` returns exactly the antecedents the recursion descended
///   into (the analysis owns `none`/unreachable filtering).
/// - `fold` receives the node and the resolved antecedent values in
///   `antecedents_of` order and computes the node's own contribution combined
///   with them. Fold operators here (AND of bools, intersection of masks) are
///   commutative and associative, so visitation order is irrelevant.
pub(crate) fn resolve_backward_dp<T, FAnts, FFold>(
    root: FlowNodeId,
    memo: &mut DpMemo<T>,
    in_progress_value: T,
    antecedents_of: FAnts,
    fold: FFold,
) -> T
where
    T: Copy,
    FAnts: Fn(FlowNodeId) -> SmallVec<[FlowNodeId; 2]>,
    FFold: Fn(FlowNodeId, &[T]) -> T,
{
    if root.is_none() {
        return in_progress_value;
    }

    // `true` marks the post-visit entry: by the time it is popped, every
    // antecedent pushed below it has already been resolved to `Done` (or left
    // `InProgress` because it is a back-edge ancestor).
    let mut stack: Vec<(FlowNodeId, bool)> = vec![(root, false)];
    while let Some((node, post)) = stack.pop() {
        if post {
            let ants = antecedents_of(node);
            let values: SmallVec<[T; 2]> = ants
                .iter()
                .map(|&a| match memo.get(&a) {
                    Some(DpState::Done(v)) => *v,
                    // `InProgress` (back-edge ancestor) or absent (`none`):
                    // contribute the no-information element.
                    _ => in_progress_value,
                })
                .collect();
            let value = fold(node, &values);
            memo.insert(node, DpState::Done(value));
            continue;
        }

        // Already resolved, or already scheduled (its post-entry is still
        // below us on the stack). Either way, do not re-expand.
        if memo.get(&node).is_some() {
            continue;
        }
        memo.insert(node, DpState::InProgress);
        stack.push((node, true));
        for antecedent in antecedents_of(node) {
            // Only descend into antecedents we have not seen. An `InProgress`
            // antecedent is a back-edge ancestor; leaving it unpushed makes the
            // post-visit read it as `in_progress_value`.
            if memo.get(&antecedent).is_none() {
                stack.push((antecedent, false));
            }
        }
    }

    match memo.get(&root) {
        Some(DpState::Done(v)) => *v,
        _ => in_progress_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Build closures over a synthetic antecedent graph. `flagged` marks the
    /// nodes carrying the searched flag; `expansions` counts how many nodes a
    /// query actually expanded, to prove memo hits across queries.
    fn graph_closures<'a>(
        edges: &'a [(u32, &'a [u32])],
        flagged: &'a [u32],
        expansions: &'a Cell<usize>,
    ) -> (
        impl Fn(FlowNodeId) -> SmallVec<[FlowNodeId; 2]> + 'a,
        impl Fn(FlowNodeId) -> bool + 'a,
    ) {
        let antecedents_of = move |node: FlowNodeId| -> SmallVec<[FlowNodeId; 2]> {
            expansions.set(expansions.get() + 1);
            edges
                .iter()
                .find(|(id, _)| *id == node.0)
                .map(|(_, ants)| ants.iter().map(|&a| FlowNodeId(a)).collect())
                .unwrap_or_default()
        };
        let is_flagged = move |node: FlowNodeId| flagged.contains(&node.0);
        (antecedents_of, is_flagged)
    }

    #[test]
    fn chain_reachability_none_root_is_false() {
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(&[], &[], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(!resolve_chain_reachability(
            FlowNodeId::NONE,
            &mut memo,
            ants,
            flag
        ));
        assert_eq!(expansions.get(), 0);
    }

    #[test]
    fn chain_reachability_finds_flag_through_linear_chain() {
        // 1(flagged) <- 2 <- 3
        let edges: &[(u32, &[u32])] = &[(3, &[2]), (2, &[1]), (1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(resolve_chain_reachability(
            FlowNodeId(3),
            &mut memo,
            &ants,
            &flag
        ));
        // The discovery path 3 -> 2 -> 1 is marked, so upstream worklist
        // queries are memo hits with no further graph expansion.
        let after_first = expansions.get();
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(1),
            &mut memo,
            &ants,
            &flag
        ));
        assert_eq!(expansions.get(), after_first);
    }

    #[test]
    fn chain_reachability_exact_on_loop_back_edge() {
        // The shape that breaks a fold-based OR DP: a switch upstream of a
        // loop. Loop header 2 has antecedents [3 (back-edge), 1 (entry)],
        // loop-body node 3 has antecedent [2], node 1 is flagged, and the
        // reference node 4 hangs off the header. A fold DP querying from 4
        // resolves 3 while 2 is still in progress and memoizes a wrong
        // `false` for 3; exact reachability must say `true` for every node.
        let edges: &[(u32, &[u32])] = &[(4, &[2]), (2, &[3, 1]), (3, &[2]), (1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        // Downstream-most node first, mirroring `check_flow` worklist order.
        assert!(resolve_chain_reachability(
            FlowNodeId(4),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(3),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(resolve_chain_reachability(
            FlowNodeId(1),
            &mut memo,
            &ants,
            &flag
        ));
    }

    #[test]
    fn chain_reachability_negative_chain_is_memoized() {
        // Diamond with a cycle and no flag anywhere:
        // 4 <- {2, 3}; 2 <- 1; 3 <- 1; 1 <- 4 (back-edge).
        let edges: &[(u32, &[u32])] = &[(4, &[2, 3]), (2, &[1]), (3, &[1]), (1, &[4])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(!resolve_chain_reachability(
            FlowNodeId(4),
            &mut memo,
            &ants,
            &flag
        ));
        let after_first = expansions.get();
        // Every node the first query visited is proven `false`; repeated
        // worklist queries are pure memo hits.
        for id in [4, 3, 2, 1] {
            assert!(!resolve_chain_reachability(
                FlowNodeId(id),
                &mut memo,
                &ants,
                &flag
            ));
        }
        assert_eq!(expansions.get(), after_first);
    }

    #[test]
    fn chain_reachability_flag_on_root_node() {
        let edges: &[(u32, &[u32])] = &[(1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(resolve_chain_reachability(
            FlowNodeId(1),
            &mut memo,
            &ants,
            &flag
        ));
    }

    #[test]
    fn chain_reachability_clear_resets_verdicts() {
        let edges: &[(u32, &[u32])] = &[(2, &[1]), (1, &[])];
        let expansions = Cell::new(0);
        let (ants, flag) = graph_closures(edges, &[1], &expansions);
        let mut memo = ChainReachabilityMemo::default();
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        memo.clear();
        let before = expansions.get();
        assert!(resolve_chain_reachability(
            FlowNodeId(2),
            &mut memo,
            &ants,
            &flag
        ));
        assert!(expansions.get() > before);
    }
}

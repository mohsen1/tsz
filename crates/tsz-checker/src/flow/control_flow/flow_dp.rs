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

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
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

        match memo.get(&node) {
            // Already resolved, or already scheduled (its post-entry is still
            // below us on the stack). Either way, do not re-expand.
            Some(_) => continue,
            None => {}
        }
        memo.insert(node, DpState::InProgress);
        stack.push((node, true));
        for antecedent in antecedents_of(node) {
            // Only descend into antecedents we have not seen. An `InProgress`
            // antecedent is a back-edge ancestor; leaving it unpushed makes the
            // post-visit read it as `in_progress_value`.
            if matches!(memo.get(&antecedent), None) {
                stack.push((antecedent, false));
            }
        }
    }

    match memo.get(&root) {
        Some(DpState::Done(v)) => *v,
        _ => in_progress_value,
    }
}

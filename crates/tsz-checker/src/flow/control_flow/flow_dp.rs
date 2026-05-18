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

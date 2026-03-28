//! Shared cycle detection utility for enum initializer evaluation.
//!
//! Both const enum evaluation (`const_enum_eval`) and non-const enum evaluation
//! (`enum_utils`) need to detect re-entrant evaluation of the same enum member.
//! This module provides a unified `CycleGuard` pattern:
//!
//! 1. A thread-local `FxHashSet<NodeIndex>` tracks members currently being evaluated.
//! 2. `try_enter` inserts a node and returns `Some(CycleGuard)` on success, or `None`
//!    if the node is already in the set (cycle detected).
//! 3. The `CycleGuard` RAII type removes the node on `Drop`, ensuring cleanup even on panic.
//!
//! Each evaluation path maintains its own thread-local set to avoid cross-contamination
//! between the declaration-checking phase (const enums) and the type-checking phase
//! (non-const enums).

use rustc_hash::FxHashSet;
use std::cell::RefCell;
use tsz_parser::parser::NodeIndex;

/// RAII guard that removes a `NodeIndex` from a thread-local visited set on drop.
///
/// Created by [`try_enter`]. When the guard goes out of scope (or the evaluation
/// panics), the tracked node is automatically removed from the visited set,
/// preventing stale entries from blocking future evaluations.
pub(crate) struct CycleGuard {
    node: NodeIndex,
    /// Which thread-local set this guard is associated with.
    set_id: CycleSetId,
}

/// Identifies which thread-local visited set a `CycleGuard` is associated with.
#[derive(Clone, Copy)]
pub(crate) enum CycleSetId {
    /// Used by `const_enum_eval` during declaration checking.
    ConstEnum,
    /// Used by `enum_utils` during type checking.
    NonConstEnum,
}

thread_local! {
    static CONST_ENUM_VISITED: RefCell<FxHashSet<NodeIndex>>
        = RefCell::new(FxHashSet::default());
    static NON_CONST_ENUM_VISITED: RefCell<FxHashSet<NodeIndex>>
        = RefCell::new(FxHashSet::default());
}

/// Clear both cycle-detection visited sets.
/// Called between compilation sessions to prevent stale NodeIndex values.
pub(crate) fn clear_visited_sets() {
    CONST_ENUM_VISITED.with(|v| v.borrow_mut().clear());
    NON_CONST_ENUM_VISITED.with(|v| v.borrow_mut().clear());
}

impl Drop for CycleGuard {
    fn drop(&mut self) {
        let node = self.node;
        match self.set_id {
            CycleSetId::ConstEnum => {
                CONST_ENUM_VISITED.with(|v| {
                    v.borrow_mut().remove(&node);
                });
            }
            CycleSetId::NonConstEnum => {
                NON_CONST_ENUM_VISITED.with(|v| {
                    v.borrow_mut().remove(&node);
                });
            }
        }
    }
}

/// Try to enter cycle detection for a node.
///
/// Returns `Some(CycleGuard)` if the node was not already being evaluated.
/// Returns `None` if the node is already in the visited set (cycle detected).
///
/// The caller must hold the returned `CycleGuard` for the duration of the
/// evaluation. When the guard is dropped, the node is removed from the set.
pub(crate) fn try_enter(node: NodeIndex, set_id: CycleSetId) -> Option<CycleGuard> {
    let already_visiting = match set_id {
        CycleSetId::ConstEnum => CONST_ENUM_VISITED.with(|v| !v.borrow_mut().insert(node)),
        CycleSetId::NonConstEnum => NON_CONST_ENUM_VISITED.with(|v| !v.borrow_mut().insert(node)),
    };
    if already_visiting {
        return None; // Cycle detected
    }
    Some(CycleGuard { node, set_id })
}

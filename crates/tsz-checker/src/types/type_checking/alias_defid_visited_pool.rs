//! Reusable scratch `FxHashSet<DefId>` pool for the type-alias resolution DFS.
//!
//! Extracted from `type_alias_checking.rs` to keep that module under the
//! 2000-line maintainability limit. The pool lets the alias-resolution DFS
//! reuse one allocation across calls; `reset_alias_defid_visited_pool` releases
//! it at independent-compilation boundaries (batch mode).

use std::cell::RefCell;
use tsz_solver::def::DefId;

// Reusable scratch `FxHashSet<DefId>` for the alias-resolution DFS.
// Mirrors the pool pattern from #4722 / #4790 and follow-up PRs.
thread_local! {
    static ALIAS_DEFID_VISITED_POOL: RefCell<Option<rustc_hash::FxHashSet<DefId>>> =
        const { RefCell::new(None) };
}

/// Run `f` with a cleared scratch `FxHashSet<DefId>` borrowed from the
/// thread-local pool, returning the set to the pool afterwards (keeping the
/// larger of the two capacities so repeated calls stop reallocating).
#[inline]
pub(crate) fn with_alias_defid_visited<R>(
    f: impl FnOnce(&mut rustc_hash::FxHashSet<DefId>) -> R,
) -> R {
    let mut visited = ALIAS_DEFID_VISITED_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_default();
    visited.clear();
    let r = f(&mut visited);
    ALIAS_DEFID_VISITED_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some(existing) => visited.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(visited);
        }
    });
    r
}

/// Drop the pooled alias-resolution scratch set.
///
/// `with_alias_defid_visited` clears the set before each use, so a retained
/// pool is never a correctness hazard mid-run, but it holds arena-scoped
/// `DefId`s and their backing capacity across compilations. Releasing it at
/// batch row boundaries keeps per-row memory from accumulating across the
/// worker's lifetime.
pub(crate) fn reset_alias_defid_visited_pool() {
    ALIAS_DEFID_VISITED_POOL.with(|p| *p.borrow_mut() = None);
}

#[cfg(test)]
pub(crate) fn set_alias_defid_visited_pool_dirty_for_test() {
    ALIAS_DEFID_VISITED_POOL.with(|p| {
        let mut set = rustc_hash::FxHashSet::default();
        set.insert(DefId::INVALID);
        *p.borrow_mut() = Some(set);
    });
}

#[cfg(test)]
pub(crate) fn alias_defid_visited_pool_is_released_for_test() -> bool {
    ALIAS_DEFID_VISITED_POOL.with(|p| p.borrow().is_none())
}

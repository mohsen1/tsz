//! Thread-local scratch buffers for predicate walkers.

use crate::TypeId;
use rustc_hash::FxHashSet;
use std::cell::RefCell;

// Reusable scratch buffers for the four predicate DFS walkers in the parent
// module (`contains_type_parameter_named`, `_shallow`, the identity-target
// variants). Each call previously allocated a fresh `FxHashSet<TypeId>` +
// `Vec<TypeId>`; pooling them in a thread-local shaves the allocator
// round-trip and the 2-4 grow reallocations. Reentrant calls (predicate from
// within another predicate's callback chain) fall through to fresh allocations
// because `take()` has already emptied the slot. Mirrors PR #4722's
// `walk_referenced_types` pool.
type PredicatePool = (FxHashSet<TypeId>, Vec<TypeId>);

thread_local! {
    static PREDICATE_POOL: RefCell<Option<PredicatePool>> = const { RefCell::new(None) };
}

#[inline]
pub(super) fn with_predicate_buffers<R>(
    f: impl FnOnce(&mut FxHashSet<TypeId>, &mut Vec<TypeId>) -> R,
) -> R {
    let mut pool = PREDICATE_POOL
        .with(|p| p.borrow_mut().take())
        .unwrap_or_else(|| (FxHashSet::default(), Vec::new()));
    pool.0.clear();
    pool.1.clear();
    let r = f(&mut pool.0, &mut pool.1);
    PREDICATE_POOL.with(|p| {
        let mut slot = p.borrow_mut();
        let keep = match &*slot {
            None => true,
            Some((existing, _)) => pool.0.capacity() >= existing.capacity(),
        };
        if keep {
            *slot = Some(pool);
        }
    });
    r
}

//! Cycle-tracking set for alias resolution, backed by [`RecursionGuard`].
//!
//! Alias resolution (`resolve_alias_symbol`, `resolve_qualified_symbol_inner`,
//! etc.) walks chains of `SymbolId`s that may cycle.  The historical
//! implementation passed a `&mut Vec<SymbolId>` through the call graph and used
//! `contains` for cycle detection.  This wrapper preserves that API — callers
//! are unaffected — but delegates the underlying set to the solver's canonical
//! [`RecursionGuard<SymbolId>`], giving O(1) cycle checks plus the profile's
//! depth and iteration caps for free.
//!
//! Semantics match the legacy Vec: [`push`] is idempotent-per-chain (a cycle
//! detected by the preceding [`contains`] check skips the push) and is **not**
//! paired with a [`pop`].  The [`RecursionGuard`] would normally panic on drop
//! with unleaved entries in debug builds; [`Drop`] here calls
//! [`RecursionGuard::reset`] first to preserve the accumulate-until-drop
//! semantics that callers rely on.

use tsz_binder::SymbolId;
use tsz_solver::recursion::{RecursionGuard, RecursionProfile, RecursionResult};

/// Depth bound preserved from the pre-migration `Vec::len()` check.
const MAX_ALIAS_RESOLUTION_DEPTH: u32 = 128;

pub(crate) struct AliasCycleTracker {
    guard: RecursionGuard<SymbolId>,
}

impl AliasCycleTracker {
    pub(crate) fn new() -> Self {
        Self {
            guard: RecursionGuard::with_profile(RecursionProfile::Custom {
                max_depth: MAX_ALIAS_RESOLUTION_DEPTH,
                max_iterations: 100_000,
            }),
        }
    }

    #[inline]
    pub(crate) fn contains(&self, sym: &SymbolId) -> bool {
        self.guard.is_visiting(sym)
    }

    /// Record `sym` as visited.  Returns `true` if the enter succeeded, `false`
    /// if the depth/iteration cap was hit or the symbol was already tracked.
    /// Callers that previously ignored `Vec::push` may ignore the result;
    /// depth is already gated by [`Self::len`].
    #[inline]
    pub(crate) fn push(&mut self, sym: SymbolId) -> bool {
        matches!(self.guard.enter(sym), RecursionResult::Entered)
    }

    /// Remove `sym` from the visiting set, mirroring the old `Vec::pop` call
    /// sites that were paired with a preceding `push`.
    #[inline]
    pub(crate) fn pop(&mut self, sym: SymbolId) {
        self.guard.leave(sym);
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.guard.depth() as usize
    }

    /// Iterate over the symbols currently tracked as visited.  Order is
    /// unspecified.  Used by callers that previously iterated `&Vec<SymbolId>`
    /// to inspect every alias seen on the chain (e.g. type-only propagation).
    pub(crate) fn iter(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.guard.visiting_iter().copied()
    }
}

impl<'a> IntoIterator for &'a AliasCycleTracker {
    type Item = SymbolId;
    type IntoIter = Box<dyn Iterator<Item = SymbolId> + 'a>;
    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.iter())
    }
}

impl Default for AliasCycleTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AliasCycleTracker {
    fn drop(&mut self) {
        // Preserve accumulate-until-drop semantics — callers don't pair every
        // `push` with a `pop`, so reset before `RecursionGuard`'s debug drop
        // check would fire on unleaved entries.
        self.guard.reset();
    }
}

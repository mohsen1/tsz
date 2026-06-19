//! Short-circuit caches for the `ensure_refs_resolved` relation-input
//! traversal (issue #13936).
//!
//! `ensure_refs_resolved` walks the transitive ref/heritage closure of every
//! relation input. Two distinct facts are worth remembering across calls, and
//! they have *different* soundness contracts, so they live in two sets:
//!
//! - `entered`: every `TypeId` ever submitted as a top-level entry. An entry is
//!   recorded even when the traversal was fuel-truncated, so its closure may be
//!   incomplete. It is only safe to use as a "don't re-enter at the top" guard
//!   — never to skip descending into a type reached transitively.
//! - `closure`: `TypeIds` whose *entire* transitive closure was resolved by a
//!   traversal that finished without exhausting either fuel budget **and** that
//!   touched only builtin-lib (`file_id == u32::MAX`) entities. Lib closures are
//!   global, bound before checking, and resolve identically in every
//!   arena/requester context, so a recorded closure is genuinely "resolved for
//!   everyone". These — and only these — are safe to skip-descend into on later
//!   traversals, which removes the repeated whole-DOM/lib heritage re-walk that
//!   dominates relation-heavy projects. Resolution being idempotent for lib
//!   types, skipping leaves the environment (and every relation verdict)
//!   byte-identical, sidestepping the #12144 under-resolution trap.
//!
//! Both share the per-file lifecycle (cleared at the file-session boundary).

use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

/// Traversal-reuse caches for `ensure_refs_resolved`. See the module docs for
/// the differing soundness contracts of the two sets.
#[derive(Debug, Default)]
pub struct RefsResolutionCache {
    entered: FxHashSet<TypeId>,
    closure: FxHashSet<TypeId>,
}

impl RefsResolutionCache {
    /// True when `type_id` was already submitted as a top-level entry or has a
    /// recorded fully-resolved lib-pure closure. Used to short-circuit a
    /// repeated top-level `ensure_refs_resolved` call.
    pub(crate) fn contains_entry_or_closure(&self, type_id: TypeId) -> bool {
        self.entered.contains(&type_id) || self.closure.contains(&type_id)
    }

    /// True when `type_id`'s full transitive lib-pure closure is already
    /// resolved into the environment. Only these types are safe to skip
    /// descending into when reached transitively.
    pub(crate) fn closure_resolved(&self, type_id: TypeId) -> bool {
        self.closure.contains(&type_id)
    }

    /// Record `type_id` as a top-level entry (closure possibly incomplete).
    pub(crate) fn mark_entered(&mut self, type_id: TypeId) {
        self.entered.insert(type_id);
    }

    /// Record `types` as having fully-resolved lib-pure closures.
    pub(crate) fn record_closures(&mut self, types: impl IntoIterator<Item = TypeId>) {
        self.closure.extend(types);
    }

    /// Clear both sets at the file-session boundary.
    pub(crate) fn clear(&mut self) {
        self.entered.clear();
        self.closure.clear();
    }
}

use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::{TypeId, TypeParamInfo};

/// File-local caches for cross-file/lib delegation helpers.
#[derive(Clone, Default)]
pub struct CrossFileDelegationCache {
    symbol_types: FxHashMap<SymbolId, (TypeId, Vec<TypeParamInfo>)>,
    declaration_node_types: Arc<dashmap::DashMap<(usize, NodeIndex, u8), TypeId>>,
    /// Session memo of completed cross-arena delegation results; shared
    /// (via `Arc` clone in `with_parent_cache`) with every transient child
    /// checker in the same file-check session. See [`CrossArenaSessionMemo`].
    session_memo: Arc<CrossArenaSessionMemo>,
}

impl CrossFileDelegationCache {
    /// Clears all caches, including the arena-stable `declaration_node_types` map.
    /// Use only when the entire checker session is torn down (not between files).
    #[inline]
    pub fn clear(&mut self) {
        self.symbol_types.clear();
        self.declaration_node_types.clear();
        // Replace rather than clear: transient child checkers from the
        // prior session may still hold Arc clones of the old memo.
        self.session_memo = Arc::new(CrossArenaSessionMemo::default());
    }

    /// Clears only the file-local `symbol_types` cache (and the file-session
    /// delegation memo, which may hold contextual sentinel outcomes that must
    /// not leak into the next file's session).
    ///
    /// `declaration_node_types` is keyed by `(arena_ptr, NodeIndex, mode)`.  Each
    /// [`NodeArena`] is a distinct heap object, so its pointer value is unique per
    /// source file.  Entries for file A cannot be returned by a lookup for file B,
    /// which means the map is safe to keep across a `switch_to_file` boundary.
    /// Preserving it avoids re-deriving cross-file declaration types on every
    /// subsequent delegation into the same foreign file.
    #[inline]
    pub fn clear_file_local(&mut self) {
        self.symbol_types.clear();
        self.session_memo = Arc::new(CrossArenaSessionMemo::default());
    }

    /// Session memo of completed cross-arena delegation results.
    #[inline]
    pub fn session_memo(&self) -> &CrossArenaSessionMemo {
        &self.session_memo
    }

    #[inline]
    pub fn symbol_type(&self, sym_id: SymbolId) -> Option<(TypeId, Vec<TypeParamInfo>)> {
        self.symbol_types.get(&sym_id).cloned()
    }

    #[inline]
    pub fn insert_symbol_type(&mut self, sym_id: SymbolId, value: (TypeId, Vec<TypeParamInfo>)) {
        self.symbol_types.insert(sym_id, value);
    }

    #[inline]
    pub fn entry_or_insert_symbol_type(
        &mut self,
        sym_id: SymbolId,
        value: (TypeId, Vec<TypeParamInfo>),
    ) {
        self.symbol_types.entry(sym_id).or_insert(value);
    }

    #[inline]
    pub fn contains_symbol_type(&self, sym_id: SymbolId) -> bool {
        self.symbol_types.contains_key(&sym_id)
    }

    #[inline]
    pub fn symbol_types(self) -> FxHashMap<SymbolId, (TypeId, Vec<TypeParamInfo>)> {
        self.symbol_types
    }

    #[inline]
    pub fn declaration_node_type(
        &self,
        arena: &NodeArena,
        decl_idx: NodeIndex,
        mode: u8,
    ) -> Option<TypeId> {
        self.declaration_node_types
            .get(&(Self::arena_ptr(arena), decl_idx, mode))
            .map(|entry| *entry)
    }

    #[inline]
    pub fn insert_declaration_node_type(
        &self,
        arena: &NodeArena,
        decl_idx: NodeIndex,
        mode: u8,
        type_id: TypeId,
    ) {
        self.declaration_node_types
            .insert((Self::arena_ptr(arena), decl_idx, mode), type_id);
    }

    #[inline]
    fn arena_ptr(arena: &NodeArena) -> usize {
        arena as *const NodeArena as usize
    }
}

/// Memo of *completed* cross-arena delegation results, scoped to one
/// outermost delegation tree.
///
/// Structural rule: when the same `(owner file, symbol)` cross-arena
/// delegation is requested repeatedly inside one delegation tree, `tsc`
/// computes the symbol's type once per program; `tsz`
/// re-ran the full child-checker pipeline per type-reference occurrence
/// whenever the completed result was a sentinel (`ERROR`/`UNKNOWN`),
/// because the shared `DefinitionStore` cross-file buckets intentionally
/// refuse sentinel writes (an *in-progress* sentinel must not poison
/// other checkers). That recomputation is the drizzle-orm livelock
/// (issue #13041): one alias such as `SelectedFields` re-ran thousands
/// of full delegations, all completing with the same `ERROR`.
///
/// This memo records *completed sentinel* outcomes only (`ERROR`/`UNKNOWN`
/// symbol results; completed-`None` class-instance/interface results),
/// keyed by `(owner_file_idx, raw symbol id, context fingerprint)`. The
/// first two components are the same global key shape the canonical
/// `DefinitionStore` cross-file buckets use: the raw `SymbolId` is
/// interpreted in the owner file's binder, so the pair is unambiguous
/// program-wide. The fingerprint is an order-independent hash of the
/// requesting checker's in-progress resolution sets
/// (`symbol_resolution_set`, `class_instance_resolution_set`,
/// `class_constructor_resolution_set`) — the mutable context a delegated
/// computation's cycle detection can observe. A completed sentinel is
/// replayed only for a repeat under the *identical* in-progress context,
/// where the baseline recomputation is the same deterministic function
/// and reproduces it. Successful results intentionally stay on the gated
/// shared-store buckets, which model requester stability; memoizing them
/// here (or replaying sentinels across *different* contexts) changed
/// elaboration output on the valibot and kysely canaries.
///
/// Scope and invalidation: one `Arc` lives on the top-level file checker
/// and is shared (not cloned) with every transient child checker via
/// `with_parent_cache`. Each memo-consuming delegation entry point clears
/// the maps when it begins a **new outermost tree**
/// ([`clear_for_new_delegation_tree`](Self::clear_for_new_delegation_tree)
/// at cross-arena depth 0), so completed sentinel outcomes are replayed
/// only *within* the delegation tree whose in-progress context produced
/// them — repeats across statements/trees recompute exactly as before
/// (zero diagnostic delta on completing canaries), while the
/// combinatorial re-resolution *inside* one tree (the livelock) collapses
/// to one computation per `(file, symbol)`. All other delegation inputs
/// (arenas, binders, merged libs, compiler options) are immutable for the
/// session, so no further invalidation is needed.
///
/// In-progress guard returns (cross-arena depth, per-checker recursion
/// budget) are *not* completed results and must never be written here.
/// Memo key: `(owner_file_idx, raw symbol id, in-progress-context fingerprint)`.
type SessionMemoKey = (u32, u32, u64);
/// Completed symbol-resolution payload: resolved type plus captured type params.
type ResolvedSymbolType = (TypeId, Vec<TypeParamInfo>);

#[derive(Default)]
pub struct CrossArenaSessionMemo {
    /// Set on every insert; lets `clear_for_new_delegation_tree` skip the
    /// per-shard map clears (the overwhelmingly common case: most outermost
    /// delegations never produce a sentinel completion).
    dirty: std::sync::atomic::AtomicBool,
    /// `delegate_cross_arena_symbol_resolution` completed sentinel results,
    /// keyed by `(owner_file_idx, raw symbol id, context fingerprint)`.
    pub symbol: dashmap::DashMap<SessionMemoKey, ResolvedSymbolType>,
    /// `delegate_cross_arena_class_instance_type` completed negative results
    /// (`None` or sentinel-typed = completed without a usable instance type).
    pub class_instance: dashmap::DashMap<SessionMemoKey, Option<ResolvedSymbolType>>,
    /// `delegate_cross_arena_interface_type` completed-`None` child-checker
    /// results (completed with `UNKNOWN`/`ERROR`).
    pub interface: dashmap::DashMap<SessionMemoKey, Option<TypeId>>,
}

impl CrossArenaSessionMemo {
    /// Mark the memo non-empty. Call after every insert into any bucket.
    #[inline]
    pub fn mark_dirty(&self) {
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Drop all entries before starting a new outermost delegation tree.
    ///
    /// Completed sentinel outcomes may depend on the in-progress resolution
    /// context of the tree that computed them; replaying them in a *later*
    /// tree (whose baseline recomputation could succeed) would change
    /// diagnostics. Call at every memo-consuming delegation entry point
    /// when the cross-arena depth is 0. Costs one relaxed atomic load when
    /// the memo is already empty.
    #[inline]
    pub fn clear_for_new_delegation_tree(&self) {
        if self.dirty.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.symbol.clear();
            self.class_instance.clear();
            self.interface.clear();
        }
    }
}

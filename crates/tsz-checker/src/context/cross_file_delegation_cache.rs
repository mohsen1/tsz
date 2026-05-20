use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::{TypeId, TypeParamInfo};

/// File-local caches for cross-file/lib delegation helpers.
#[derive(Clone)]
pub struct CrossFileDelegationCache {
    symbol_types: FxHashMap<SymbolId, (TypeId, Vec<TypeParamInfo>)>,
    declaration_node_types: Arc<dashmap::DashMap<(usize, NodeIndex, u8), TypeId>>,
}

impl Default for CrossFileDelegationCache {
    fn default() -> Self {
        Self {
            symbol_types: FxHashMap::default(),
            declaration_node_types: Arc::new(dashmap::DashMap::new()),
        }
    }
}

impl CrossFileDelegationCache {
    /// Clears all caches, including the arena-stable `declaration_node_types` map.
    /// Use only when the entire checker session is torn down (not between files).
    #[inline]
    pub fn clear(&mut self) {
        self.symbol_types.clear();
        self.declaration_node_types.clear();
    }

    /// Clears only the file-local `symbol_types` cache.
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

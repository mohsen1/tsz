use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_parser::parser::{NodeArena, NodeIndex};
use tsz_solver::DefId;
use tsz_solver::{TypeId, TypeParamInfo};

/// File-local caches for cross-file/lib delegation helpers.
#[derive(Clone)]
pub struct CrossFileDelegationCache {
    symbol_types: FxHashMap<SymbolId, (TypeId, Vec<TypeParamInfo>)>,
    declaration_node_types: Arc<dashmap::DashMap<(usize, NodeIndex, u8), TypeId>>,
    actual_lib_def_ids: RefCell<FxHashMap<String, Option<DefId>>>,
}

impl Default for CrossFileDelegationCache {
    fn default() -> Self {
        Self {
            symbol_types: FxHashMap::default(),
            declaration_node_types: Arc::new(dashmap::DashMap::new()),
            actual_lib_def_ids: RefCell::new(FxHashMap::default()),
        }
    }
}

impl CrossFileDelegationCache {
    #[inline]
    pub fn clear(&mut self) {
        self.symbol_types.clear();
        self.declaration_node_types.clear();
        self.clear_actual_lib_def_ids();
    }

    #[inline]
    pub fn clear_actual_lib_def_ids(&self) {
        self.actual_lib_def_ids.borrow_mut().clear();
    }

    #[inline]
    pub fn actual_lib_def_id(&self, name: &str) -> Option<Option<DefId>> {
        self.actual_lib_def_ids.borrow().get(name).copied()
    }

    #[inline]
    pub fn insert_actual_lib_def_id(&self, name: String, value: Option<DefId>) {
        self.actual_lib_def_ids.borrow_mut().insert(name, value);
    }

    #[inline]
    pub fn actual_lib_def_ids_is_empty(&self) -> bool {
        self.actual_lib_def_ids.borrow().is_empty()
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
            .get(&(arena as *const NodeArena as usize, decl_idx, mode))
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
        self.declaration_node_types.insert(
            (arena as *const NodeArena as usize, decl_idx, mode),
            type_id,
        );
    }
}
